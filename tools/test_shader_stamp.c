// The shader stamp rule (runtime/shader_stamp.c): which change makes the shaders prepare again before play. No game files.
//   clang -std=gnu11 -O1 tools/test_shader_stamp.c runtime/shader_stamp.c -o build/test-shader-stamp.exe && build/test-shader-stamp.exe
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#ifdef _WIN32
#include <direct.h>
#define mkdir(p, m) _mkdir(p)
#endif

#include "../runtime/shader_stamp.h"

static const ShaderStampIn BASE = {
    .runtime = "0.4.0 translator 3fa9c01b22de cache 1", .exe = "17640960 1764000000",
    .gpu = "NVIDIA GeForce RTX 3050 Laptop GPU, vendor 10de device 25a2, driver 2400000000, cache 0a1b2c3d",
    .list = "9e3779b97f4a7c15", .code = "d41d8cd9-D9BA7DB72FFAFECD 118980608 1764000100", .mods = "b60fps/atextures",
};

static int failed;
static void check(bool ok, const char *what) {
    if (!ok) fprintf(stderr, "FAIL: %s\n", what), failed = 1;
}

static void expect_field(const ShaderStampIn *now, const char *want_field, const char *what) {
    char have[2048], want[2048], field[64] = "";
    shader_stamp_format(have, sizeof have, &BASE);
    shader_stamp_format(want, sizeof want, now);
    bool differs = shader_stamp_diff(have, want, field, sizeof field);
    if (!want_field) check(!differs, what);
    else check(differs && !strcmp(field, want_field), what);
    if (want_field && differs && strcmp(field, want_field)) fprintf(stderr, "  %s: field \"%s\", want \"%s\"\n", what, field, want_field);
}

int main(void) {
    ShaderStampIn n;
    expect_field(&BASE, NULL, "the same setup matches at once");
    n = BASE, n.gpu = "NVIDIA GeForce RTX 3050 Laptop GPU, vendor 10de device 25a2, driver 2401000000, cache 0a1b2c3d";
    expect_field(&n, "gpu", "a driver version change prepares again");
    n = BASE, n.gpu = "AMD Radeon RX 7800 XT, vendor 1002 device 747e, driver 8388653, cache ffee";
    expect_field(&n, "gpu", "another GPU prepares again");
    n = BASE, n.list = "9e3779b97f4a7c16";
    expect_field(&n, "list", "a new shipped list prepares again");
    n = BASE, n.code = "d41d8cd9-D9BA7DB72FFAFECD 118980608 1764000999";
    expect_field(&n, "code", "a game code rebuild prepares again");
    n = BASE, n.exe = "17640961 1764000000";
    expect_field(&n, "exe", "another runtime file prepares again");
    n = BASE, n.runtime = "0.4.1 translator 3fa9c01b22de cache 1";
    expect_field(&n, "runtime", "a new version prepares again");
    n = BASE, n.mods = "b60fps";
    expect_field(&n, "mods", "a mod switched off prepares again");
    n = BASE, n.mods = "";
    expect_field(&n, "mods", "no mods prepares again");

    char want[2048], field[64];
    shader_stamp_format(want, sizeof want, &BASE);
    check(shader_stamp_diff(NULL, want, field, sizeof field) && !strcmp(field, "none"), "no stamp file prepares");
    check(shader_stamp_diff("", want, field, sizeof field) && !strcmp(field, "none"), "an empty stamp file prepares");
    check(shader_stamp_diff("runtime 0.4.0\n", want, field, sizeof field) && !strcmp(field, "shaders-prepared"), "a foreign file prepares");
    // a stamp of an older layout, without the code line, is stale rather than accepted
    char old[2048];
    snprintf(old, sizeof old, "shaders-prepared 1\nruntime %s\nexe %s\ngpu %s\nlist %s\nmods %s\n", BASE.runtime, BASE.exe, BASE.gpu,
             BASE.list, BASE.mods);
    check(shader_stamp_diff(old, want, field, sizeof field) && !strcmp(field, "code"), "a stamp missing a field prepares");
    // a field name is not matched by another that starts with it
    check(shader_stamp_diff("shaders-prepared 1\nruntime x\nexe y\ngpuX z\n", want, field, sizeof field), "gpuX is not gpu");
    // the stamp read back from disk, with and without a trailing newline cut off, matches itself
    mkdir("build", 0755);
    char path[128];
    snprintf(path, sizeof path, "build/test-shader-stamp-%u.txt", (unsigned)rand());
    check(shader_stamp_write(path, want), "the stamp is written");
    char back[2048];
    check(shader_stamp_read(path, back, sizeof back) && !shader_stamp_diff(back, want, field, sizeof field), "the stamp read back matches");
    check(shader_stamp_write(path, "x\n") && shader_stamp_read(path, back, sizeof back) && !strcmp(back, "x\n"), "a stamp is replaced");
    remove(path);
    check(!shader_stamp_read(path, back, sizeof back), "a missing stamp file reads as none");
    puts(failed ? "shader stamp: FAILED" : "shader stamp: ok");
    return failed;
}
