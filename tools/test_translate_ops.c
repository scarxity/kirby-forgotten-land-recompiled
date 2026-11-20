// Instruction-semantics regression test for the Maxwell shader translator (runtime/nvn/maxwell/translate.c).
//
// Each case is a tiny compute program: MOV32I loads the inputs, then the instruction under test. Its MSL body
// (the GLSL is the same text after glsl_rewrite) is written into a C++ file with stand-ins for the few MSL
// built-ins it uses, compiled and run on the CPU, and the registers / predicates are compared with values worked
// out by hand from the instruction's definition. tools/test_translate_ops.sh drives it.
//
// --glsl <dir>: instead, writes each case's Vulkan GLSL (key.glsl = 1) as a vertex, fragment and compute shader
// (<dir>/<case>_<stage>.{vert,frag,comp}, the case names in <dir>/names.txt) for tools/test_spirv_val.sh, which compiles
// them (glslc, Vulkan 1.3) and runs spirv-val on each. (The shipped shader list holds only hashes of the game's
// shaders - their code is in the player's game files - so CI validates these synthetic programs instead.)
//
// usage: test_translate_ops <out.cpp>
//        test_translate_ops --glsl <dir>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../runtime/nvn/maxwell/shader.h"

#define PT_GUARD (7ull << 16)  // @PT
#define NOP 0x50b0000000070f00ull
#define EXIT 0xe30000000007000full

static uint64_t mov32i(int d, uint32_t v) { return 0x0100000000000000ull | (uint64_t)v << 20 | 0xFull << 12 | PT_GUARD | (uint64_t)d; }

// I2I Rd, Rb (register form). Sizes: 0 8-bit, 1 16-bit, 2 32-bit.
static uint64_t i2i(int d, int b, int dsz, bool dsg, int ssz, bool ssg, bool sat, bool abs_, bool neg) {
    return 0x5ce0000000000000ull | PT_GUARD | (uint64_t)d | (uint64_t)dsz << 8 | (uint64_t)ssz << 10 | (uint64_t)dsg << 12 |
           (uint64_t)ssg << 13 | (uint64_t)b << 20 | (uint64_t)neg << 45 | (uint64_t)abs_ << 49 | (uint64_t)sat << 50;
}

// SHF.{L,R} Rd, Ra (low), shift, Rc (high). width: 0 32-bit, 2 .U64, 3 .S64. shift_reg < 0: immediate `imm`.
static uint64_t shf(bool right, int d, int a, int shift_reg, uint32_t imm, int c, int width, bool wrap) {
    uint64_t w = shift_reg >= 0 ? (right ? 0x5cf8000000000000ull : 0x5bf8000000000000ull) | (uint64_t)shift_reg << 20
                                : (right ? 0x38f8000000000000ull : 0x36f8000000000000ull) | (uint64_t)(imm & 63) << 20;
    return w | PT_GUARD | (uint64_t)d | (uint64_t)a << 8 | (uint64_t)width << 37 | (uint64_t)c << 39 | (uint64_t)wrap << 50;
}

// HSETP2 Pa, Pb, Ra, Rb (register form, both halves), compare `cmp`, combined with predicate (p, pneg) by `bop`
static uint64_t hsetp2(int pa, int pb, int a, int b, int cmp, int p, bool pneg, int bop, bool hand) {
    return 0x5d20000000000000ull | PT_GUARD | (uint64_t)pb | (uint64_t)pa << 3 | (uint64_t)a << 8 | (uint64_t)b << 20 |
           (uint64_t)cmp << 35 | (uint64_t)p << 39 | (uint64_t)pneg << 42 | (uint64_t)bop << 45 | (uint64_t)hand << 49;
}

// LDC Rd, c[idx][Ra + off]; size 4 is 32-bit, 5 is .64 (Rd, Rd+1)
static uint64_t ldc(int d, int a, int idx, uint32_t off, int size) {
    return 0xef90000000000000ull | PT_GUARD | (uint64_t)d | (uint64_t)a << 8 | (uint64_t)(off & 0xFFFF) << 20 |
           (uint64_t)idx << 36 | (uint64_t)size << 48;
}

// LDS / STS Rd, [Ra + off]: shared memory (compute only). Sizes: 0 U8, 1 S8, 2 U16, 3 S16, 4 32-bit, 5 .64, 6 .128
static uint64_t lds(int d, int a, uint32_t off, int size) {
    return 0xef48000000000000ull | PT_GUARD | (uint64_t)d | (uint64_t)a << 8 | (uint64_t)(off & 0xFFFFFF) << 20 | (uint64_t)size << 48;
}
static uint64_t sts(int d, int a, uint32_t off, int size) { return lds(d, a, off, size) | 1ull << 52; }

// VMAD Rd, Ra.<asel>, Rb.<bsel> (b < 0: immediate imm), Rc / VSETP.<cmp>.AND Pd, PT, Ra.<asel>, Rb.<bsel>, PT. A
// selector: 0..3 bytes (width byte), 4..5 halves, 6 the word; bit 3 of it: signed
static uint64_t vsel(int sel, int sel_lo, int width_lo) {  // (selector and width share their middle bit)
    int s = sel & 7, width = s < 4 ? (s >> 1) : s < 6 ? 2 : 3, k = s < 4 ? (s & 3) : s - 4;
    return (uint64_t)(k & 1) << sel_lo | (uint64_t)width << width_lo;
}
static uint64_t video(uint64_t op, int a, int asel, int b, int bsel, uint32_t imm) {
    uint64_t w = op | PT_GUARD | (uint64_t)a << 8 | vsel(asel, 36, 37) | (uint64_t)(asel >> 3) << 48;
    if (b >= 0) w |= 1ull << 50 | (uint64_t)b << 20 | vsel(bsel, 28, 29) | (uint64_t)(bsel >> 3) << 49;
    else w |= (uint64_t)(imm & 0xFFFF) << 20 | (uint64_t)(bsel >> 3) << 49;
    return w;
}
static uint64_t vmad(int d, int a, int asel, int b, int bsel, uint32_t imm, int c) {
    return video(0x5f00000000000000ull, a, asel, b, bsel, imm) | (uint64_t)d | (uint64_t)c << 39;
}
static uint64_t vsetp(int p, int q, int cmp, int a, int asel, int b, int bsel, uint32_t imm) {
    return video(0x50f0000000000000ull, a, asel, b, bsel, imm) | (uint64_t)q | (uint64_t)p << 3 | 7ull << 39 |
           (uint64_t)(cmp & 3) << 43 | (uint64_t)(cmp >> 2) << 47;
}
static uint64_t isberd(int d, int a) { return 0xefd0000000000000ull | PT_GUARD | (uint64_t)d | (uint64_t)a << 8; }

typedef struct {
    const char *name;
    uint64_t insn[6];  // 0-terminated
    const char *var;   // the register / predicate to check
    uint32_t expect;
    bool compute_only;
} Case;

enum { LT = 1, GT = 4, AND = 0, OR = 1 };
#define LO 0x89ABCDEFu
#define HI 0x01234567u

static const Case *cases(int *n) {
    static Case c[64];
    int k = 0;
#define CASE(nm, var_, exp_, ...) c[k++] = (Case){nm, {__VA_ARGS__, 0}, var_, exp_, false}
#define CASE_CS(nm, var_, exp_, ...) c[k++] = (Case){nm, {__VA_ARGS__, 0}, var_, exp_, true}
    // I2I: .SAT compares in the source's signedness and saturates 32-bit destinations too
    CASE("I2I.U8.U32.SAT 0xffffffff", "r0", 0xFFu, mov32i(1, 0xFFFFFFFFu), i2i(0, 1, 0, false, 2, false, true, false, false));
    CASE("I2I.U16.U32.SAT 0x12345", "r0", 0xFFFFu, mov32i(1, 0x12345u), i2i(0, 1, 1, false, 2, false, true, false, false));
    CASE("I2I.S16.U32.SAT 0x80000000", "r0", 0x7FFFu, mov32i(1, 0x80000000u), i2i(0, 1, 1, true, 2, false, true, false, false));
    CASE("I2I.S32.U32.SAT 0x80000000", "r0", 0x7FFFFFFFu, mov32i(1, 0x80000000u), i2i(0, 1, 2, true, 2, false, true, false, false));
    CASE("I2I.U32.S32.SAT -1", "r0", 0u, mov32i(1, 0xFFFFFFFFu), i2i(0, 1, 2, false, 2, true, true, false, false));
    CASE("I2I.U32.S32.SAT 5", "r0", 5u, mov32i(1, 5u), i2i(0, 1, 2, false, 2, true, true, false, false));
    CASE("I2I.S8.S32.SAT -200", "r0", 0xFFFFFF80u, mov32i(1, (uint32_t)-200), i2i(0, 1, 0, true, 2, true, true, false, false));
    CASE("I2I.U16.S32.SAT 70000", "r0", 0xFFFFu, mov32i(1, 70000u), i2i(0, 1, 1, false, 2, true, true, false, false));
    CASE("I2I.S8.S32 0x1ff (zero-extended)", "r0", 0xFFu, mov32i(1, 0x1FFu), i2i(0, 1, 0, true, 2, true, false, false, false));
    CASE("I2I.S32.S32 |-5|", "r0", 5u, mov32i(1, (uint32_t)-5), i2i(0, 1, 2, true, 2, true, false, true, false));
    // SHF: hi:lo funnel; .W wraps the shift, otherwise it's clamped to the width (32, or 64 for .U64 / .S64)
    CASE("SHF.R.W 4", "r0", 0x789ABCDEu, mov32i(1, LO), mov32i(2, HI), mov32i(3, 4), shf(true, 0, 1, 3, 0, 2, 0, true));
    CASE("SHF.R.W 36 (wraps to 4)", "r0", 0x789ABCDEu, mov32i(1, LO), mov32i(2, HI), mov32i(3, 36), shf(true, 0, 1, 3, 0, 2, 0, true));
    CASE("SHF.R 40 (clamps to 32)", "r0", HI, mov32i(1, LO), mov32i(2, HI), mov32i(3, 40), shf(true, 0, 1, 3, 0, 2, 0, false));
    CASE("SHF.L 40 (clamps to 32)", "r0", LO, mov32i(1, LO), mov32i(2, HI), mov32i(3, 40), shf(false, 0, 1, 3, 0, 2, 0, false));
    CASE("SHF.L.W 8", "r0", 0x23456789u, mov32i(1, LO), mov32i(2, HI), mov32i(3, 8), shf(false, 0, 1, 3, 0, 2, 0, true));
    CASE("SHF.R.U64 imm 36", "r0", 0x00123456u, mov32i(1, LO), mov32i(2, HI), shf(true, 0, 1, -1, 36, 2, 2, false));
    CASE("SHF.R.U64 imm 4", "r0", 0x789ABCDEu, mov32i(1, LO), mov32i(2, HI), shf(true, 0, 1, -1, 4, 2, 2, false));
    CASE("SHF.R.U64 70 (clamps to 64)", "r0", 0u, mov32i(1, LO), mov32i(2, HI), mov32i(3, 70), shf(true, 0, 1, 3, 0, 2, 2, false));
    CASE("SHF.R.S64 40", "r0", 0xFFF00000u, mov32i(1, 0), mov32i(2, 0xF0000000u), mov32i(3, 40), shf(true, 0, 1, 3, 0, 2, 3, false));
    CASE("SHF.R.S64 70 (clamps to 64)", "r0", 0xFFFFFFFFu, mov32i(1, 0), mov32i(2, 0xF0000000u), mov32i(3, 70), shf(true, 0, 1, 3, 0, 2, 3, false));
    CASE("SHF.L.U64 40", "r0", 0xABCDEF00u, mov32i(1, LO), mov32i(2, HI), mov32i(3, 40), shf(false, 0, 1, 3, 0, 2, 2, false));
    CASE("SHF.L.U64 64", "r0", 0u, mov32i(1, LO), mov32i(2, HI), mov32i(3, 64), shf(false, 0, 1, 3, 0, 2, 2, false));
    CASE("SHF.L.U64.W 72 (wraps to 8)", "r0", 0x23456789u, mov32i(1, LO), mov32i(2, HI), mov32i(3, 72), shf(false, 0, 1, 3, 0, 2, 2, true));
    CASE("SHF.L.U64 imm 40", "r0", 0xABCDEF00u, mov32i(1, LO), mov32i(2, HI), shf(false, 0, 1, -1, 40, 2, 2, false));
    // HSETP2.H_AND: both halves are combined with the predicate, then ANDed; the second result is the complement
    CASE("HSETP2.GT.H_AND.AND !PT (first)", "p0", 0, mov32i(1, 0x3C003C00u), mov32i(2, 0), hsetp2(0, 1, 1, 2, GT, 7, true, AND, true));
    CASE("HSETP2.GT.H_AND.AND !PT (second)", "p1", 1, mov32i(1, 0x3C003C00u), mov32i(2, 0), hsetp2(0, 1, 1, 2, GT, 7, true, AND, true));
    CASE("HSETP2.GT.H_AND.OR PT (first)", "p0", 1, mov32i(1, 0x00003C00u), mov32i(2, 0), hsetp2(0, 1, 1, 2, GT, 7, false, OR, true));
    CASE("HSETP2.GT.H_AND.OR PT (second)", "p1", 0, mov32i(1, 0x00003C00u), mov32i(2, 0), hsetp2(0, 1, 1, 2, GT, 7, false, OR, true));
    CASE("HSETP2.LT.AND PT (per half)", "p1", 1, mov32i(1, 0xBC000000u), mov32i(2, 0), hsetp2(0, 1, 1, 2, LT, 7, false, AND, false));
    // LDC.64 with the address register among the destinations: both words come from the original address (the
    // test's constant buffer 3 holds 0x1000 + word index at every word)
    CASE("LDC.64 R4, c[3][R4+4] (low)", "r4", 0x1003u, mov32i(4, 8), ldc(4, 4, 3, 4, 5));
    CASE("LDC.64 R4, c[3][R4+4] (high)", "r5", 0x1004u, mov32i(4, 8), ldc(4, 4, 3, 4, 5));
    CASE("LDC.64 R4, c[3][R5+4] (low)", "r4", 0x1003u, mov32i(5, 8), ldc(4, 5, 3, 4, 5));
    CASE("LDC.64 R4, c[3][R5+4] (high)", "r5", 0x1004u, mov32i(5, 8), ldc(4, 5, 3, 4, 5));
    CASE("LDC R4, c[3][R4+4]", "r4", 0x1003u, mov32i(4, 8), ldc(4, 4, 3, 4, 4));
    // LDS / STS: one array of words shared by the workgroup; sub-word stores keep the word's other bytes
    CASE_CS("STS [R2+4] / LDS [20]", "r0", 0x12345678u, mov32i(1, 0x12345678u), mov32i(2, 16), sts(1, 2, 4, 4), lds(0, 255, 20, 4));
    CASE_CS("STS.U8 into a stored word", "r0", 0xAABB11DDu, mov32i(1, 0xAABBCCDDu), sts(1, 255, 8, 4), mov32i(3, 0x11), sts(3, 255, 9, 0), lds(0, 255, 8, 4));
    CASE_CS("LDS.S16 sign-extends the high half", "r0", 0xFFFF8000u, mov32i(1, 0x8000FFFFu), sts(1, 255, 0, 4), lds(0, 255, 2, 3));
    CASE_CS("STS.64 / LDS.64 (high)", "r1", 2u, mov32i(4, 1), mov32i(5, 2), sts(4, 255, 0x40, 5), lds(0, 255, 0x40, 5));
    // VMAD / VSETP: the geometry stages' vertex numbers from the invocation info word (vertices in byte 2)
    CASE("VMAD R0 = R1.B0 * R1.B2 + R2", "r0", 16u, mov32i(1, 0x00030005u), mov32i(2, 1), vmad(0, 1, 0, 1, 2, 0, 2));
    CASE("VMAD R0 = R1.H1 * 6 + RZ", "r0", 42u, mov32i(1, 0x00070000u), vmad(0, 1, 5, -1, 0, 6, 255));
    CASE("VMAD R0 = R1.S8 (byte 3) * -2 (signed imm)", "r0", 2u, mov32i(1, 0xFF000000u), vmad(0, 1, 8 | 3, -1, 8, 0xFFFE, 255));
    CASE("VMAD R0 = R1 (word) * R2.B1 + R3", "r0", 0x10000017u, mov32i(1, 0x08000000u), mov32i(2, 0x0200u), mov32i(3, 0x17u),
         vmad(0, 1, 6, 2, 1, 0, 3));
    CASE("VSETP.LE R1.B2, 2 (3 vertices)", "p0", 0, mov32i(1, 0x00030000u), vsetp(0, 7, 3, 1, 2, -1, 0, 2));
    CASE("VSETP.LE R1.B2, 3", "p0", 1, mov32i(1, 0x00030000u), vsetp(0, 7, 3, 1, 2, -1, 0, 3));
    CASE("VSETP.GT R1.B0, R2.B2 (second)", "p1", 1, mov32i(1, 1), mov32i(2, 0x00050000u), vsetp(0, 1, 4, 1, 0, 2, 2, 0));
    CASE("ISBERD passes the vertex through", "r0", 2u, mov32i(1, 2), isberd(0, 1));
#undef CASE_CS
#undef CASE
    *n = k;
    return c;
}

// C++ stand-ins for the MSL built-ins the bodies use (only the ones these instructions emit)
static const char prelude[] =
    "#include <cstdint>\n#include <cstdio>\n#include <cstring>\n#include <cstdlib>\n"
    "typedef uint32_t uint; typedef uint64_t ulong;\n"
    "#define long int64_t\n"
    "#define threadgroup\n"
    "template <class T, class U> static T as_type(U u) { static_assert(sizeof(T) == sizeof(U), \"size\"); T t; memcpy(&t, &u, sizeof t); return t; }\n"
    "template <class T> static T min(T a, T b) { return b < a ? b : a; }\n"
    "template <class T> static T max(T a, T b) { return a < b ? b : a; }\n"
    "template <class T> static T clamp(T x, T lo, T hi) { return min(max(x, lo), hi); }\n"
    "struct float2 { float x, y; };\n"
    "static float h2f(uint h) { _Float16 f; uint16_t b = (uint16_t)h; memcpy(&f, &b, 2); return (float)f; }\n"
    "static float2 hx(uint v, uint swz) { if (swz == 1u) { float f = as_type<float>(v); return {f, f}; }"
    " float2 h = {h2f(v & 0xFFFFu), h2f(v >> 16)}; return swz == 2u ? float2{h.x, h.x} : swz == 3u ? float2{h.y, h.y} : h; }\n"
    "static float2 operator-(float2 a) { return {-a.x, -a.y}; }\n"
    "static uint bfe_u(uint v, uint ctl) { uint pos = ctl & 0xFFu, len = (ctl >> 8) & 0xFFu; return (v >> pos) & (len >= 32u ? ~0u : (1u << len) - 1u); }\n"
    "static uint bfe_s(uint v, uint ctl) { uint len = (ctl >> 8) & 0xFFu, x = bfe_u(v, ctl); return len < 32u && (x >> (len - 1u)) & 1u ? x | ~((1u << len) - 1u) : x; }\n"
    "struct cbuf { uint at(uint off) const { return 0x1000u + (off >> 2); } };\n"
    "static const cbuf cb3 = {};\n"
    "static uint cbr(const cbuf &cb, uint off) { return cb.at(off); }\n"
    "static int fails;\n"
    "static void check(const char *name, uint got, uint want) {\n"
    "    if (got != want) { printf(\"FAIL %s: got 0x%08x, want 0x%08x\\n\", name, got, want); fails++; }\n"
    "}\n";

// The case's program: its instructions with scheduling words, EXIT, padded with NOPs; returns the word count
static int program(const Case *c, uint64_t prog[16]) {
    int m = 0;
    for (const uint64_t *w = c->insn; *w; w++) {
        if (m % 4 == 0) prog[m++] = 0;  // scheduling word
        prog[m++] = *w;
    }
    if (m % 4 == 0) prog[m++] = 0;
    prog[m++] = EXIT;
    while (m % 4) prog[m++] = NOP;
    return m;
}

static int write_glsl(const char *dir) {
    static const struct { int stage; const char *ext; } st[] = {
        {STAGE_VERTEX, "vert"}, {STAGE_FRAGMENT, "frag"}, {STAGE_COMPUTE, "comp"}};
    char path[1024];
    snprintf(path, sizeof path, "%s/names.txt", dir);
    FILE *names = fopen(path, "w");
    if (!names) { perror(path); return 2; }
    int n, bad = 0;
    const Case *cs = cases(&n);
    for (int i = 0; i < n; i++) {
        uint64_t prog[16];
        int m = program(&cs[i], prog);
        for (int j = 0; j < 3; j++) {
            if (cs[i].compute_only && st[j].stage != STAGE_COMPUTE) continue;
            // vertex / fragment: the 0x80-byte program header first (zero, as tools/test_translate.c's; fragment:
            // color target 0 written, output map at 0x30 + 0x48)
            uint8_t buf[0x80 + sizeof prog] = {0};
            size_t hdr = st[j].stage == STAGE_COMPUTE ? 0 : 0x80;
            if (st[j].stage == STAGE_FRAGMENT) buf[0x30 + 0x48] = 0xF;
            memcpy(buf + hdr, prog, (size_t)m * 8);
            ShaderKey key = {0};  // (compute: as the Vulkan backend's key, exec.c translate_compute)
            key.glsl = 1;
            ShaderInfo *s = maxwell_translate(buf, hdr + (size_t)m * 8, st[j].stage, &key);
            fprintf(names, "%02d_%s %s\n", i, st[j].ext, cs[i].name);
            if (!s->msl) {
                printf("FAIL %s (%s): translation: %s\n", cs[i].name, st[j].ext, s->error);
                bad++;
            } else {
                snprintf(path, sizeof path, "%s/%02d_%s.%s", dir, i, st[j].ext, st[j].ext);
                FILE *f = fopen(path, "w");
                if (!f) { perror(path); return 2; }
                fputs(s->msl, f);
                fclose(f);
            }
            maxwell_free(s);
        }
    }
    {   // (compute, GLSL) a load and a store through global memory: the fast-copy table lookups gfind / gfindw and the
        // store report (exec.c gmap_build, gfast_flags_poll) - compiled and validated like the cases (#355)
        const Case gc = {.name = "LDG / STG (fast-copy table, store report)",
                         .insn = {mov32i(2, 0x10000), mov32i(3, 0x72),
                                  0xeed0000000000000ull | 4ull << 48 | 1ull << 45 | PT_GUARD | 2ull << 8 | 4,  // LDG.E R4, [R2]
                                  0xeed0000000000000ull | 1ull << 51 | 4ull << 48 | 1ull << 45 | 16ull << 20 | PT_GUARD | 2ull << 8 | 4,  // STG.E [R2+16], R4
                                  0}};
        uint64_t prog[16];
        int m = program(&gc, prog);
        uint64_t gb = maxwell_gmem_base, gs = maxwell_gmem_size;
        maxwell_gmem_base = 0x7200000000ull, maxwell_gmem_size = 0x400000000ull;
        ShaderKey key = {0};
        key.glsl = 1;
        ShaderInfo *s = maxwell_translate((const uint8_t *)prog, (size_t)m * 8, STAGE_COMPUTE, &key);
        maxwell_gmem_base = gb, maxwell_gmem_size = gs;
        fprintf(names, "%02d_comp %s\n", n, gc.name);
        if (!s->msl || !strstr(s->msl, "gfindw(")) {
            printf("FAIL %s: %s\n", gc.name, s->msl ? "no gfindw at the store" : s->error);
            bad++;
        } else {
            snprintf(path, sizeof path, "%s/%02d_comp.comp", dir, n);
            FILE *f = fopen(path, "w");
            if (!f) { perror(path); return 2; }
            fputs(s->msl, f);
            fclose(f);
        }
        maxwell_free(s);
    }
    {   // (GLSL) RED.E.ADD.S32 [R2+16], R4 and RED.E.OR [R2], R4: atomics on global memory, through the fast copy table in
        // compute, the heap in a vertex shader (the car Kirby geometry stages' kind)
        uint64_t red = 0xebf8000000000000ull | 1ull << 48 | PT_GUARD | 2ull << 8 | 4;
        const Case rc = {.name = "RED.E.ADD / RED.E.OR", .insn = {mov32i(2, 0x10000), mov32i(3, 0x72), mov32i(4, 1),
                                                                    red | 1ull << 20 | 16ull << 28, red | 6ull << 23, 0}};
        uint64_t prog[16];
        int m = program(&rc, prog);
        uint8_t buf[0x80 + sizeof prog] = {0};
        memcpy(buf + 0x80, prog, (size_t)m * 8);
        uint64_t gb = maxwell_gmem_base, gs = maxwell_gmem_size;
        maxwell_gmem_base = 0x7200000000ull, maxwell_gmem_size = 0x400000000ull;
        static const struct { int stage; const char *ext, *want; size_t hdr; } st[] = {
            {STAGE_COMPUTE, "comp", "if (fw != 0xFFFFFFFFu) atomicAdd(gfast_.w[fw], r4); else if (ok) GMATOM_atomicAdd(uint(ga >> 2), r4);", 0},
            {STAGE_VERTEX, "vert", "#define GMATOM_atomicOr(i, v) atomicOr(gmem_.w[i], v)", 0x80}};
        for (int j = 0; j < 2; j++) {
            ShaderKey key = {0};
            key.glsl = 1;
            ShaderInfo *s = maxwell_translate(st[j].hdr ? buf : (const uint8_t *)prog, st[j].hdr + (size_t)m * 8, st[j].stage, &key);
            fprintf(names, "%02d_%s %s\n", n + 3 + j, st[j].ext, rc.name);
            if (!s->msl || !strstr(s->msl, st[j].want) || !s->uses_gmem_store) {
                printf("FAIL %s (%s): %s\n", rc.name, st[j].ext, s->msl ? "not the atomic" : s->error);
                bad++;
            } else {
                snprintf(path, sizeof path, "%s/%02d_%s.%s", dir, n + 3 + j, st[j].ext, st[j].ext);
                FILE *f = fopen(path, "w");
                if (!f) { perror(path); return 2; }
                fputs(s->msl, f);
                fclose(f);
            }
            maxwell_free(s);
        }
        maxwell_gmem_base = gb, maxwell_gmem_size = gs;
    }
    {   // (compute, GLSL) SUST.P.3D through a bindless handle LDC loaded from c[2] (image 1: byte 0x128) becomes an
        // imageStore on the image slot of that handle; a handle the code computes is refused, not guessed
        uint64_t sust = 0xeb20000000000000ull | PT_GUARD | 0xFull << 20 | 5ull << 33 | 7ull << 39 | 4ull << 8 | 0;  // [R4..R6], R0..R3, handle R7
        const Case ok = {.name = "SUST.P.3D, handle from c[2]", .insn = {mov32i(4, 1), ldc(7, 255, 2, 0x128, 4), mov32i(0, 0x3f800000), sust, 0}};
        const Case no = {.name = "SUST.P.3D, computed handle", .insn = {mov32i(7, 0x128), sust, 0}};
        uint64_t prog[16];
        ShaderKey key = {0};
        key.glsl = 1;
        ShaderInfo *s = maxwell_translate((const uint8_t *)prog, (size_t)program(&ok, prog) * 8, STAGE_COMPUTE, &key);
        fprintf(names, "%02d_comp %s\n", n + 1, ok.name);
        if (!s->msl || !strstr(s->msl, "uniform writeonly image3D t0;") || !strstr(s->msl, "imageStore(t0, ivec3(") ||
            s->ntex != 1 || !s->tex[0].is_image || s->tex[0].handle_offset != 0x128) {
            printf("FAIL %s: %s\n", ok.name, s->msl ? "not an imageStore on the handle's image" : s->error);
            bad++;
        } else {
            snprintf(path, sizeof path, "%s/%02d_comp.comp", dir, n + 1);
            FILE *f = fopen(path, "w");
            if (!f) { perror(path); return 2; }
            fputs(s->msl, f);
            fclose(f);
        }
        maxwell_free(s);
        s = maxwell_translate((const uint8_t *)prog, (size_t)program(&no, prog) * 8, STAGE_COMPUTE, &key);
        if (s->msl || !strstr(s->error, "image handle")) { printf("FAIL %s: %s\n", no.name, s->msl ? "translated" : s->error); bad++; }
        maxwell_free(s);
    }
    {   // (geometry, GLSL) a pass-through triangle as the game's geometry stages start: the vertex number from the
        // invocation info (VMAD), ISBERD, ALD of position and attribute 0 at that vertex, AST, OUT emit then cut
        uint64_t ald = 0xefd8000000000000ull | PT_GUARD | 255ull << 8 | 1ull << 39, ast = 0xeff0000000000000ull | PT_GUARD | 255ull << 8;
        uint64_t out = 0xfbe0000000000000ull | PT_GUARD | 255ull << 20;
        const uint64_t insn[] = {0xf0c8000000000000ull | PT_GUARD | 0x1dull << 20, vmad(1, 0, 0, 0, 2, 0, 255), isberd(1, 1),
                                 ald | 3ull << 47 | 0x70ull << 20 | 4, ald | 0x80ull << 20 | 8, ast | 3ull << 47 | 0x70ull << 20 | 4,
                                 ast | 0x80ull << 20 | 8, out | 1ull << 39, out | 2ull << 39, EXIT};
        uint8_t buf[0x80 + 8 * 32] = {0};
        uint32_t sph[5] = {1u | 4u << 10, 0, 1u << 24, 7u << 24, 3};  // (geometry; 1 invocation, triangle strip, 3 vertices)
        memcpy(buf + 0x30, sph, sizeof sph);
        int m = 0;
        uint64_t prog[32];
        for (size_t i = 0; i < sizeof insn / sizeof insn[0]; i++) {
            if (m % 4 == 0) prog[m++] = 0;
            prog[m++] = insn[i];
        }
        while (m % 4) prog[m++] = NOP;
        memcpy(buf + 0x80, prog, (size_t)m * 8);
        ShaderKey key = {0};
        key.glsl = 1, key.flip_y = 1, key.vs_out_mask = 1;
        ShaderInfo *s = maxwell_translate(buf, 0x80 + (size_t)m * 8, STAGE_GEOMETRY, &key);
        const char *want[] = {"layout(triangles) in;", "layout(triangle_strip, max_vertices = 3) out;", "in vec4 gin0[];",
                              "out vec4 v0;", "gl_in[min(r1, 2u)].gl_Position[3]", "gin0[min(r1, 2u)][0]",
                              "gl_Position.y = -gl_Position.y;\n    EmitVertex();", "EndPrimitive();"};
        const char *miss = NULL;
        for (size_t i = 0; s->msl && !miss && i < sizeof want / sizeof want[0]; i++) if (!strstr(s->msl, want[i])) miss = want[i];
        fprintf(names, "%02d_geom geometry pass-through\n", n + 2);
        if (!s->msl || miss || s->attr_mask != 1 || s->out_mask != 1) {
            printf("FAIL geometry pass-through: %s%s\n", s->msl ? "no " : s->error, s->msl ? (miss ? miss : "attr / out masks") : "");
            bad++;
        } else {
            snprintf(path, sizeof path, "%s/%02d_geom.geom", dir, n + 2);
            FILE *f = fopen(path, "w");
            if (!f) { perror(path); return 2; }
            fputs(s->msl, f);
            fclose(f);
        }
        maxwell_free(s);
        key.vs_out_mask = 0;  // (attribute 0 not from the vertex shader: a constant, no input)
        s = maxwell_translate(buf, 0x80 + (size_t)m * 8, STAGE_GEOMETRY, &key);
        if (!s->msl || s->attr_mask || strstr(s->msl, "gin0")) { printf("FAIL geometry input the vertex shader lacks: %s\n", s->msl ? "declared" : s->error); bad++; }
        maxwell_free(s);
    }
    {   // (vertex, GLSL) ShaderInfo.int_reads: attribute 1 into FMUL (a float input), attribute 2 through a MOV into
        // IADD (an integer one, as a skinned model's bone index), attribute 3 into nothing
        uint64_t ald = 0xefd8000000000000ull | PT_GUARD | 255ull << 8 | 255ull << 39;
        uint64_t mov = 0x5c98000000000000ull | PT_GUARD | 0xfull << 39, iadd = 0x5c10000000000000ull | PT_GUARD,
                 fmul = 0x5c68000000000000ull | PT_GUARD;
        const uint64_t insn[] = {ald | 0x90ull << 20 | 0, ald | 0xa0ull << 20 | 1, ald | 0xb0ull << 20 | 5,
                                 mov | 1ull << 20 | 2, iadd | 2ull << 20 | 2ull << 8 | 3, fmul | 0ull << 20 | 0ull << 8 | 4, EXIT};
        uint8_t buf[0x80 + 8 * 16] = {0};
        uint32_t sph[5] = {1u | 1u << 10, 0, 0, 0, 0};  // (vertex)
        memcpy(buf + 0x30, sph, sizeof sph);
        buf[0x30 + 0x18] = 0xf0, buf[0x30 + 0x19] = 0xff;  // (inputs: attributes 1, 2 and 3)
        int m = 0;
        uint64_t prog[16];
        for (size_t i = 0; i < sizeof insn / sizeof insn[0]; i++) {
            if (m % 4 == 0) prog[m++] = 0;
            prog[m++] = insn[i];
        }
        while (m % 4) prog[m++] = NOP;
        memcpy(buf + 0x80, prog, (size_t)m * 8);
        ShaderKey key = {0};
        key.glsl = 1;
        ShaderInfo *s = maxwell_translate(buf, 0x80 + (size_t)m * 8, STAGE_VERTEX, &key);
        if (!s->msl || s->int_reads != 1u << 2) {
            printf("FAIL vertex inputs read as integers: %s %llx (want 4)\n", s->msl ? "int_reads" : s->error, (unsigned long long)s->int_reads);
            bad++;
        }
        maxwell_free(s);
    }
    fclose(names);
    printf("%d cases x 3 stages written to %s, %d translation failures\n", n, dir, bad);
    return bad != 0;
}

int main(int argc, char **argv) {
    if (argc == 3 && !strcmp(argv[1], "--glsl")) return write_glsl(argv[2]);
    if (argc < 2) { fprintf(stderr, "usage: %s <out.cpp> | --glsl <dir>\n", argv[0]); return 2; }
    FILE *o = fopen(argv[1], "w");
    if (!o) { perror(argv[1]); return 2; }
    fputs(prelude, o);
    int n, bad = 0;
    const Case *cs = cases(&n);
    for (int i = 0; i < n; i++) {
        uint64_t prog[16];
        int m = program(&cs[i], prog);
        ShaderKey key = {0};
        ShaderInfo *s = maxwell_translate((const uint8_t *)prog, (size_t)m * 8, STAGE_COMPUTE, &key);
        const char *b = s->msl ? strstr(s->msl, "kernel void main0(") : NULL;
        b = b ? strstr(b, "{\n") : NULL;
        const char *e = b ? strstr(b, "  return;\n}") : NULL;
        if (!e) { printf("FAIL %s: translation: %s\n", cs[i].name, s->msl ? "unexpected layout" : s->error); bad++; continue; }
        fprintf(o, "static void case%d() %.*s  check(\"%s\", (uint)%s, 0x%08xu);\n}\n", i, (int)(e - b), b, cs[i].name, cs[i].var,
                cs[i].expect);
        maxwell_free(s);
    }
    fprintf(o, "int main() {\n");
    for (int i = 0; i < n; i++) fprintf(o, "    case%d();\n", i);
    fprintf(o, "    printf(\"%d cases, %%d failures\\n\", fails);\n    return fails != 0;\n}\n", n);
    fclose(o);
    return bad != 0;
}
