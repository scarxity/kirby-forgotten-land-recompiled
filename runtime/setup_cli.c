// The launchers' command line (runtime/setup_cli.h, docs/launcher-runtime-contract.md).
#include "setup_cli.h"

#include <SDL3/SDL.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#ifdef _WIN32
#include <windows.h>
#endif

#include "../game/game.h"
#include "game_addresses.h"
#include "gamebuild.h"
#include "gamelib.h"
#include "paths.h"
#include "platform.h"
#include "romfs_build.h"
#include "rt.h"

#define ROOT "extracted"  // (in the data root, the working directory)

// A failed setup's exit code for --prepare (RecompLauncher's recomplauncher.json exit_codes) and its reason for the others
enum { NOT_FOUND = 8, PACKAGE, VERSION, COPY, BUILD, TOOLS, BAD_IMAGE, UPDATE };
static const char *const REASON[] = {"not_found", "package", "version", "copy", "build", "tools", "bad_image", "update"};

enum { LOOKING, COPYING, READING, COMPILING, FINISHING, SHADERS };
static const char *const PHASE[] = {"looking", "copying", "reading", "compiling", "finishing", "shaders"};
static const char *const PREPARE_TEXT[] = {"Looking for the game files", "Copying the game files", "Reading the game code",
                                           "Compiling the game code", "Finishing", "Preparing shaders"};

static char g_cmd[16], g_folder[4096];

static bool is_file(const char *p) {
    struct stat st;
    return stat(p, &st) == 0 && S_ISREG(st.st_mode);
}

bool setup_cli_parse(int argc, char **argv) {
    for (int i = 1; i < argc; i++) {
        const char *a = argv[i];
        if (!strcmp(a, "--status") || !strcmp(a, "--rebuild-code") || !strcmp(a, "--build-romfs")) {
            snprintf(g_cmd, sizeof g_cmd, "%s", a + 2);
        } else if ((!strcmp(a, "--check") || !strcmp(a, "--setup") || !strcmp(a, "--prepare")) && i + 1 < argc) {
            snprintf(g_cmd, sizeof g_cmd, "%s", a + 2);
            if (!realpath(argv[i + 1], g_folder)) snprintf(g_folder, sizeof g_folder, "%s", argv[i + 1]);  // (resolved now: rt_paths_init changes directory)
            for (char *c = g_folder; *c; c++) if (*c == '\\') *c = '/';
            i++;
        }
    }
    if (!g_cmd[0]) return false;
#ifdef _WIN32
    // (a GUI-subsystem process started with no stderr would get a session log that takes stdout too: plat_init)
    HANDLE err = GetStdHandle(STD_ERROR_HANDLE);
    if (!err || err == INVALID_HANDLE_VALUE || GetFileType(err) == FILE_TYPE_UNKNOWN) freopen("NUL", "ab", stderr);
#endif
    if (!strcmp(g_cmd, "prepare") && !getenv("LARECOMP_DATA_ROOT")) {  // RecompLauncher: next to the exe, wherever it was started from
        char dir[1024];
        char *s = plat_exe_path(dir, sizeof dir) ? strrchr(dir, '\\') : NULL;
        if (!s && plat_exe_path(dir, sizeof dir)) s = strrchr(dir, '/');
        if (s) *s = 0, setenv("LARECOMP_DATA_ROOT", dir, 1);
    }
    return true;
}

// ------------------------------------------------------------------ what the game's files are
static uint64_t program_id(const char *exefs) {
    char p[2300];
    snprintf(p, sizeof p, "%s/main.npdm", exefs);
    FILE *f = fopen(p, "rb");
    if (!f) return 0;
    uint64_t id = 0;
    uint8_t meta[0x80], aci[0x18];  // ACI0 (offset at META+0x70): the program id at +0x10
    if (fread(meta, 1, sizeof meta, f) == sizeof meta && !memcmp(meta, "META", 4) &&
        !fseek(f, *(uint32_t *)(meta + 0x70), SEEK_SET) && fread(aci, 1, sizeof aci, f) == sizeof aci && !memcmp(aci, "ACI0", 4))
        id = *(uint64_t *)(aci + 0x10);
    fclose(f);
    return id;
}

// The RomFS set up in ROOT: the folder the game is served from, or an image (a romfs.bin picked instead of the files)
static bool romfs_set_up(void) {
    char why[128];
    return romfs_is_game(ROOT "/romfs") || (is_file(ROOT "/romfs.bin") && romfs_image_ok(ROOT "/romfs.bin", why, sizeof why));
}
static bool files_set_up(void) { return program_id(ROOT "/exefs") == GAME_PROGRAM_ID && romfs_set_up(); }

typedef struct { char exefs[2200], romfs[2200], image[2200]; bool package; } Found;

// The search and the verdict, shared by every command that takes a folder: 0 when the folder holds the game, else the failure
static int find_verdict(const char *folder, Found *f) {
    romfs_find_game_image(folder, f->exefs, sizeof f->exefs, f->romfs, sizeof f->romfs, f->image, sizeof f->image, &f->package);
    LOG("[setup] %s: exefs %s, romfs %s, romfs.bin %s\n", folder, f->exefs[0] ? f->exefs : "-", f->romfs[0] ? f->romfs : "-",
        f->image[0] ? f->image : "-");
    if (!f->exefs[0] || (!f->romfs[0] && !f->image[0])) return f->package ? PACKAGE : NOT_FOUND;
    // (the picked folder can't be our own copy: setting up again deletes that first)
    char a[4096], b[4096];
    if (realpath(f->exefs, a) && realpath(ROOT, b)) {
        size_t n = strlen(b);
        if (!strncmp(a, b, n) && (a[n] == '/' || a[n] == '\\' || !a[n])) return NOT_FOUND;
    }
    uint64_t id = program_id(f->exefs);
    if (id == GAME_UPDATE_PROGRAM_ID) return UPDATE;
    char label[32];
    if (id != GAME_PROGRAM_ID || !game_release_label(f->exefs, label, sizeof label)) {
        LOG("[setup] %s: program id %016llx, its main not a version this runtime supports\n", f->exefs,
            (unsigned long long)id);
        return VERSION;
    }
    return 0;
}

// ------------------------------------------------------------------ the lines
static bool g_prepare;
// "phase <name> <0..1> [(<slow file>, <n> min)]", or for RecompLauncher "progress <0..1> <text>": only when something changed
static void emit(int phase, double fraction, const char *slow) {
    static int last_phase = -1, last_permille = -1;
    static char last_slow[200];
    int permille = (int)(fraction * 1000);
    if (phase == last_phase && permille == last_permille && !strcmp(slow, last_slow)) return;
    last_phase = phase, last_permille = permille;
    snprintf(last_slow, sizeof last_slow, "%s", slow);
    if (g_prepare) printf("progress %.3f %s%s%s\n", permille / 1000.0, PREPARE_TEXT[phase], *slow ? " " : "", slow);
    else printf("phase %s %.3f%s%s\n", PHASE[phase], permille / 1000.0, *slow ? " " : "", slow);
    fflush(stdout);
}

// Copying is the first 15% of the whole, the game code (gamebuild_run) the rest
static void copy_progress(uint64_t done, uint64_t total) {
    if (total) emit(COPYING, 0.01 + 0.14 * ((double)done / (double)total), "");
}
static void build_progress(double f, const char *step) {
    const char *slow = strchr(step, '(');  // (gamebuild.c adds the file the compiler has been on longest, once it is slow)
    int phase = !strncmp(step, "Finishing", 9) ? FINISHING
                : !strncmp(step, "Recompiling", 11) || !strncmp(step, "Resuming", 8) ? COMPILING : READING;
    // (--prepare goes on to the shaders: the game code is 15-70% of its bar there)
    emit(phase, 0.15 + (g_prepare ? 0.55 : 0.85) * f, slow ? slow : "");
}

// --prepare (RecompLauncher, which starts the game straight after it) also prepares the shaders, as the Tauri launcher
// does before Play: without this step every draw's pipeline is built during play, 20-30 fps with freezes of up to 2.4 s
// in the first stage (RTX 3050 Laptop). The same hidden play run the Tauri launcher makes (launcher main.rs
// run_shaders); its "phase shaders <0..1>" lines become the last 30% of the bar. A failure leaves play possible, so the
// setup still succeeds.
static void prepare_shaders(void) {
    char exe[1024], cmd[1200], line[256];
    if (!plat_exe_path(exe, sizeof exe)) return;
    static const char *const vars[] = {"LARECOMP_HIDDEN", "LARECOMP_NO_USER_INPUT", "LARECOMP_SHADER_PREP",
                                       "LARECOMP_SHADER_PREBUILD", "LARECOMP_SHADER_PREP_EXIT"};
    for (size_t i = 0; i < sizeof vars / sizeof *vars; i++) setenv(vars[i], "1", 1);
#ifdef _WIN32
    snprintf(cmd, sizeof cmd, "\"\"%s\" " ROOT " --play\"", exe);  // (cmd.exe strips the outer quotes)
#else
    snprintf(cmd, sizeof cmd, "'%s' " ROOT " --play", exe);
#endif
    emit(SHADERS, 0.70, "");
    FILE *p = popen(cmd, "r");
    if (!p) { LOG("[setup] shaders: could not start the game to prepare them\n"); return; }
    double f;
    while (fgets(line, sizeof line, p))
        if (sscanf(line, "phase shaders %lf", &f) == 1) emit(SHADERS, 0.70 + 0.30 * f, "");
    int st = pclose(p);
    LOG("[setup] shaders: %s\n", st ? "not prepared (they are built during play instead)" : "prepared");
}

// ------------------------------------------------------------------ the setup
static int build_code(void) {
    if (!gamebuild_available()) return TOOLS;
    int cores = SDL_GetNumLogicalCPUCores(), ram_gb = SDL_GetSystemRAM() / 1024;
    GameBuildOpts o = {.progress = build_progress, .jobs = cores < ram_gb - 2 ? cores : ram_gb - 2 > 1 ? ram_gb - 2 : 1};
    char why[256] = "";
    if (!gamebuild_run(ROOT, &o, why, sizeof why)) {
        LOG("[setup] game code: %s\n", why);
        return BUILD;
    }
    return gamelib_present(ROOT) ? 0 : BUILD;
}

// Everything the setup makes in ROOT but its build cache (.gamebuild): the next build reuses what is still good
// (an update compiles only the changed files, a cancelled build resumes), and checks every piece against its inputs.
static bool remove_setup(void) {
    static const char *const made[] = {"exefs", "romfs", "romfs.bin", GAMELIB_NAME, GAMELIB_NAME ".id"};
    bool ok = true;
    for (size_t i = 0; i < sizeof made / sizeof *made; i++) ok &= rt_remove_game_file(ROOT, made[i]);
    return ok;
}

// exefs/ in ROOT from the folder found: the folder itself, or - a flat dump, main next to msg/ and gfx/ - only its code files
#define MOD(n, base) #n,
static bool copy_exefs(const Found *f) {
    if (strcmp(f->exefs, f->romfs)) return romfs_copy_tree(f->exefs, ROOT "/exefs", NULL);
    mkdir(ROOT "/exefs", 0755);
    static const char *const code[] = {GAME_MODULES(MOD) "main.npdm", "rtld"};
    for (size_t i = 0; i < sizeof code / sizeof *code; i++) {
        char src[2300], dst[1100];
        snprintf(src, sizeof src, "%s/%s", f->exefs, code[i]);
        snprintf(dst, sizeof dst, ROOT "/exefs/%s", code[i]);
        if (is_file(src) ? !romfs_copy_file(src, dst) : strcmp(code[i], "rtld") != 0) return false;  // (rtld: not every dump has it)
    }
    return true;
}
#undef MOD

// A RomFS that can't be read (a folder tree that can't be walked, an image cut short) is refused before the earlier
// setup is removed. The search already found the folder by the game's marker folders.
static bool romfs_readable(const Found *f) {
    char why[256] = "";
    if (!f->romfs[0]) {
        bool ok = romfs_image_ok(f->image, why, sizeof why);
        if (!ok) LOG("[setup] the game data image %s: %s\n", f->image, why);
        return ok;
    }
    RomfsLayout l;
    bool ok = romfs_layout(f->romfs, &l, why, sizeof why);
    romfs_layout_free(&l);
    if (!ok) LOG("[setup] %s: %s\n", f->romfs, why);
    return ok;
}

static int setup(bool rebuild_only) {
    if (rebuild_only) {  // (the game files set up already, the game code from another version of the app)
        LOG("[setup] rebuilding the game code from the game files set up already\n");
        return files_set_up() ? build_code() : NOT_FOUND;
    }
    emit(LOOKING, 0, "");
    Found f = {0};
    int r = find_verdict(g_folder, &f);
    if (r) return r;
    if (!romfs_readable(&f)) return BAD_IMAGE;
    mkdir(ROOT, 0755);
    if (!remove_setup()) return COPY;
    mkdir(ROOT, 0755);
    emit(COPYING, 0, "");
    if (!copy_exefs(&f) || !(f.romfs[0] ? romfs_copy_tree(f.romfs, ROOT "/romfs", copy_progress)
                                        : romfs_copy_file(f.image, ROOT "/romfs.bin")))
        return COPY;
    return build_code();
}

int setup_cli_run(void) {
    if (!strcmp(g_cmd, "check")) {
        Found f = {0};
        int r = find_verdict(g_folder, &f);
        if (r) printf("failed %s\n", REASON[r - NOT_FOUND]);
        else printf("ok\n");
        fflush(stdout);
        return r != 0;
    }
    if (!strcmp(g_cmd, "status")) {
        printf("%s\n", !files_set_up() ? "none" : gamelib_present(ROOT) ? "ready" : "build");
        fflush(stdout);
        return 0;
    }
    if (!strcmp(g_cmd, "build-romfs")) {  // (the RomFS is served from its folder: no image to build; only checks there is one)
        if (romfs_set_up()) return 0;
        fprintf(stderr, "romfs image: no game RomFS (a romfs folder or a romfs.bin image) in %s\n", ROOT);
        return 1;
    }
    g_prepare = !strcmp(g_cmd, "prepare");
    int r = setup(!strcmp(g_cmd, "rebuild-code"));
    if (g_prepare && !r) prepare_shaders();
    LOG("[setup] %s\n", r ? REASON[r - NOT_FOUND] : "ready");
    if (g_prepare) {
        if (!r) printf("progress 1 Ready\n");
    } else if (r) {
        printf("failed %s\n", REASON[r - NOT_FOUND]);
    } else {
        printf("done\n");
    }
    fflush(stdout);
    return !r ? 0 : g_prepare ? r : 1;
}
