// LARECOMP_FRAMETIME_CSV=<path> (tests): one row per window present, for frame pacing and Frame Generation's
// in-betweens (tools/frametime_summary.py summarises it).
//   present   the game frame the image belongs to
//   kind      game (the game's own image), mid (a Frame Generation in-between), repeat (a frame shown again)
//   t_ms      when vkQueuePresentKHR was called (from the first row), dt_ms the time since the previous row's call
//   call_ms   how long vkQueuePresentKHR took
//   shown_ms  when the image reached the display (VK_KHR_present_wait: -1 without it, or when it never showed),
//             shown_dt_ms the time since the previous row's
// The call times are the app's own spacing; only shown_ms says what the display did with it (FIFO's refresh grid).
// vkWaitForPresentKHR needs the swapchain externally synchronised, so it is polled (timeout 0) under the same lock as
// the present: the shown time is accurate to the poll's 0.25 ms.
#include <SDL3/SDL.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>

#include "vk.h"
#include "../../platform.h"

typedef struct { VkSwapchainKHR sc; uint64_t id, frame, t_call, t_ret; int kind; } FtRow;
#define FT_Q 256
static FtRow q[FT_Q];
static unsigned q_head, q_tail;  // (pending rows: tail..head, under lock)
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static FILE *csv;
static bool waiting;  // (present wait enabled on the device)
static uint64_t next_id = 1, t_first, last_call, last_shown;
static PFN_vkWaitForPresentKHR wait_fn;

bool ft_wanted(void) { const char *p = getenv("LARECOMP_FRAMETIME_CSV"); return p && *p; }

static void write_row(const FtRow *r, int64_t shown) {
    static const char *const kinds[] = {"game", "mid", "repeat"};
    double t = (r->t_call - t_first) / 1e6, dt = last_call ? (r->t_call - last_call) / 1e6 : 0;
    double s = shown >= 0 ? (shown - (int64_t)t_first) / 1e6 : -1;
    double sdt = shown >= 0 && last_shown ? (shown - (int64_t)last_shown) / 1e6 : -1;
    fprintf(csv, "%llu,%s,%.3f,%.3f,%.3f,%.3f,%.3f\n", (unsigned long long)r->frame, kinds[r->kind], t, dt,
            (r->t_ret - r->t_call) / 1e6, s, sdt);
    last_call = r->t_call;
    if (shown >= 0) last_shown = (uint64_t)shown;
    static unsigned n;
    if (++n % 120 == 0) fflush(csv);
}

static void *waiter(void *arg) {
    (void)arg;
    for (;;) {
        pthread_mutex_lock(&lock);
        while (q_tail != q_head) {
            FtRow *r = &q[q_tail % FT_Q];
            VkResult res = r->sc ? wait_fn(g_vk.dev, r->sc, r->id, 0) : VK_ERROR_OUT_OF_DATE_KHR;
            if (res == VK_TIMEOUT) {
                if (plat_now_ns() - r->t_ret < 500000000ull) break;  // (never shown in half a second: dropped)
                res = VK_ERROR_OUT_OF_DATE_KHR;
            }
            write_row(r, res == VK_SUCCESS || res == VK_SUBOPTIMAL_KHR ? (int64_t)plat_now_ns() : -1);
            q_tail++;
        }
        pthread_mutex_unlock(&lock);
        SDL_DelayPrecise(250000);
    }
    return NULL;
}

void ft_init(bool present_wait) {
    const char *path = getenv("LARECOMP_FRAMETIME_CSV");
    if (!path || !*path || csv) return;
    csv = fopen(path, "w");
    if (!csv) { LOG("[vk] can't open the frame-time CSV %s\n", path); return; }
    fputs("present,kind,t_ms,dt_ms,call_ms,shown_ms,shown_dt_ms\n", csv);
    wait_fn = present_wait ? (PFN_vkWaitForPresentKHR)vkGetDeviceProcAddr(g_vk.dev, "vkWaitForPresentKHR") : NULL;
    waiting = wait_fn != NULL;
    LOG("[vk] frame-time CSV %s (display times %s)\n", path, waiting ? "from present wait" : "not available");
    if (waiting) { pthread_t t; pthread_create(&t, NULL, waiter, NULL); }
}

// Around vkQueuePresentKHR (the recorder): ft_present_lock chains a present id into pi; ft_present_done queues the row.
void ft_present_lock(VkPresentInfoKHR *pi, VkPresentIdKHR *pid, uint64_t *id) {
    if (!csv) return;
    pthread_mutex_lock(&lock);
    *id = 0;
    if (!waiting) return;
    *id = next_id++;
    *pid = (VkPresentIdKHR){.sType = VK_STRUCTURE_TYPE_PRESENT_ID_KHR, .pNext = pi->pNext, .swapchainCount = 1, .pPresentIds = id};
    pi->pNext = pid;
}
void ft_present_done(VkSwapchainKHR sc, uint64_t id, int kind, uint64_t frame, uint64_t t_call, uint64_t t_ret, bool ok) {
    if (!csv) return;
    if (!t_first) t_first = t_call;
    FtRow r = {ok ? sc : VK_NULL_HANDLE, id, frame, t_call, t_ret, kind};
    if (!waiting) write_row(&r, -1);
    else {
        if (q_head - q_tail == FT_Q) write_row(&q[q_tail++ % FT_Q], -1);  // (full: the oldest goes without its shown time)
        q[q_head++ % FT_Q] = r;
    }
    pthread_mutex_unlock(&lock);
}
// Before a swapchain is destroyed: its pending rows are written without a shown time.
void ft_swapchain_gone(VkSwapchainKHR sc) {
    if (!csv || !waiting) return;
    pthread_mutex_lock(&lock);
    for (unsigned i = q_tail; i != q_head; i++) if (q[i % FT_Q].sc == sc) q[i % FT_Q].sc = VK_NULL_HANDLE;
    pthread_mutex_unlock(&lock);
}
