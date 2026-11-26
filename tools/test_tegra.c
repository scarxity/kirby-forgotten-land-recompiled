// Block-linear deswizzle (runtime/nvn/tegra.c): the GOB-wise copy gives the bytes the per-piece reference gives, for
// block heights and depths, widths that are not a whole number of GOB columns, and 3D slices. With a size argument it
// also times both on one large surface (ns per MB).
#include "../runtime/nvn/tegra.c"
#undef NDEBUG
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static uint64_t rng = 88172645463325252ull;
static uint32_t rnd(uint32_t n) {
    rng ^= rng << 13, rng ^= rng >> 7, rng ^= rng << 17;
    return (uint32_t)(rng % n);
}

static uint64_t surface_size(uint32_t wb, uint32_t hb, uint32_t d, uint32_t bpb, uint32_t bh, uint32_t bd) {
    uint64_t width_gobs = div_up(wb * bpb, 64);
    return width_gobs * 512 * (1u << bh) * (1u << bd) * div_up(hb, 8u << bh) * div_up(d, 1u << bd);
}

static double now_s(void) {
    struct timespec t;
    timespec_get(&t, TIME_UTC);
    return t.tv_sec + t.tv_nsec * 1e-9;
}

int main(int argc, char **argv) {
    static const uint32_t bpbs[] = {1, 2, 3, 4, 8, 12, 16};
    for (int it = 0; it < 3000; it++) {
        uint32_t bpb = bpbs[rnd(7)], wb = 1 + rnd(it % 10 ? 70 : 300), hb = 1 + rnd(it % 10 ? 90 : 300);
        uint32_t bh = rnd(5), bd = rnd(3), d = rnd(4) ? 1 : 1 + rnd(6);
        if (d == 1) bd = 0;
        uint64_t n = surface_size(wb, hb, d, bpb, bh, bd), lin = (uint64_t)wb * bpb * hb * d;
        uint8_t *src = malloc(n), *a = malloc(lin), *b = malloc(lin);
        for (uint64_t i = 0; i < n; i++) src[i] = (uint8_t)(rnd(256));
        copy_surface(src, a, wb, hb, d, bpb, bh, bd, true);
        tegra_deswizzle(src, b, wb, hb, d, bpb, bh, bd);
        if (memcmp(a, b, lin)) {
            printf("MISMATCH wb %u hb %u d %u bpb %u bh %u bd %u\n", wb, hb, d, bpb, bh, bd);
            return 1;
        }
        free(src), free(a), free(b);
    }
    printf("deswizzle: 3000 random surfaces match the reference\n");
    if (argc > 1) {
        uint32_t wb = 512, hb = 512, bpb = 16, bh = 4, reps = atoi(argv[1]);
        uint64_t n = surface_size(wb, hb, 1, bpb, bh, 0), lin = (uint64_t)wb * bpb * hb;
        uint8_t *src = malloc(n), *dst = malloc(lin);
        for (uint64_t i = 0; i < n; i++) src[i] = (uint8_t)i;
        double t0 = now_s();
        for (uint32_t r = 0; r < reps; r++) copy_surface(src, dst, wb, hb, 1, bpb, bh, 0, true);
        double t1 = now_s();
        for (uint32_t r = 0; r < reps; r++) tegra_deswizzle(src, dst, wb, hb, 1, bpb, bh, 0);
        double t2 = now_s();
        printf("%.1f MB surface: per-piece %.2f GB/s, GOB-wise %.2f GB/s\n", n / 1048576.0, n * (double)reps / (t1 - t0) / 1e9,
               n * (double)reps / (t2 - t1) / 1e9);
    }
    return 0;
}
