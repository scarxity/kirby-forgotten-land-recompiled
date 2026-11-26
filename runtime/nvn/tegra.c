// Tegra X1 block-linear layout. A GOB is 64 bytes x 8 rows; blocks stack GOBs vertically
// (block height) and in depth (block depth). Levels shrink their block height as they shrink.
#include "tegra.h"

#include <string.h>

static uint32_t div_up(uint32_t a, uint32_t b) { return (a + b - 1) / b; }
static uint64_t align_up(uint64_t a, uint64_t b) { return (a + b - 1) / b * b; }

uint32_t tegra_default_block_height_log2(uint32_t height_in_blocks) {
    uint32_t gobs = div_up(height_in_blocks, 8);
    uint32_t log2 = 0;
    while ((1u << log2) < gobs && log2 < 4) log2++;
    return log2;
}

static uint32_t block_depth_for(uint32_t depth) {
    uint32_t log2 = 0;
    while ((1u << log2) < depth && log2 < 5) log2++;
    return log2;
}

void tegra_layout_init(TexLayout *t) {
    uint32_t hb = div_up(t->height, t->fmt->bh);
    t->block_height_log2 = tegra_default_block_height_log2(hb);
    t->block_depth_log2 = t->depth > 1 ? block_depth_for(t->depth) : 0;
}

// Block height shrinks with the level so a block never exceeds the level height.
static uint32_t level_block_height(uint32_t bh_log2, uint32_t hb) {
    while (bh_log2 > 0 && (8u << (bh_log2 - 1)) >= hb) bh_log2--;
    return bh_log2;
}

static uint32_t level_block_depth(uint32_t bd_log2, uint32_t d) {
    while (bd_log2 > 0 && (1u << (bd_log2 - 1)) >= d) bd_log2--;
    return bd_log2;
}

static uint64_t level_size(const TexLayout *t, uint32_t level, LevelLayout *out) {
    uint32_t w = t->width >> level, h = t->height >> level, d = t->depth >> level;
    if (!w) w = 1;
    if (!h) h = 1;
    if (!d) d = 1;
    uint32_t wb = div_up(w, t->fmt->bw), hb = div_up(h, t->fmt->bh);
    uint32_t bh = level_block_height(t->block_height_log2, hb);
    uint32_t bd = level_block_depth(t->block_depth_log2, d);
    uint32_t gob_rows = 8u << bh, gob_depth = 1u << bd;
    uint64_t width_gobs = div_up(wb * t->fmt->bpb, 64);
    uint64_t size = width_gobs * 512 * (1u << bh) * (1u << bd) * div_up(hb, gob_rows) * div_up(d, gob_depth);
    if (out) {
        out->w = w; out->h = h; out->d = d;
        out->wb = wb; out->hb = hb;
        out->block_height_log2 = bh;
        out->block_depth_log2 = bd;
    }
    return size;
}

LevelLayout tegra_level(const TexLayout *t, uint32_t level) {
    LevelLayout l = {0};
    uint64_t off = 0;
    for (uint32_t i = 0; i < level; i++) off += level_size(t, i, NULL);
    level_size(t, level, &l);
    l.offset = off;
    return l;
}

uint64_t tegra_layer_size(const TexLayout *t) {
    uint64_t size = 0;
    for (uint32_t i = 0; i < t->levels; i++) size += level_size(t, i, NULL);
    if (t->layers > 1) {
        // layers are aligned to the level-0 block size
        uint32_t bh = t->block_height_log2;
        size = align_up(size, 512ull << bh);
    }
    return size;
}

uint64_t tegra_total_size(const TexLayout *t) {
    return tegra_layer_size(t) * (t->layers ? t->layers : 1);
}

// Offset of byte (x bytes, y rows, z slices) within a block-linear surface.
static inline uint64_t gob_offset(uint32_t xb, uint32_t y, uint32_t z, uint32_t width_gobs, uint32_t bh_log2,
                                  uint32_t bd_log2, uint64_t slice_blocks_rows) {
    uint32_t gob_x = xb >> 6, gob_y = y >> 3;
    uint32_t block_y = gob_y >> bh_log2, gob_in_block_y = gob_y & ((1u << bh_log2) - 1);
    uint32_t block_z = z >> bd_log2, z_in_block = z & ((1u << bd_log2) - 1);
    uint64_t block_size = 512ull << (bh_log2 + bd_log2);
    uint64_t off = ((uint64_t)block_z * slice_blocks_rows + block_y) * width_gobs * block_size;
    off += (uint64_t)gob_x * block_size;
    off += ((uint64_t)z_in_block << bh_log2 | gob_in_block_y) * 512;
    // within the GOB: 64x8 bytes arranged in 16-byte sectors
    uint32_t xi = xb & 63, yi = y & 7;
    off += ((xi >> 5) << 8) | ((yi >> 1) << 6) | (((xi >> 4) & 1) << 5) | ((yi & 1) << 4) | (xi & 0x0F);
    return off;
}

static void copy_surface(const uint8_t *src, uint8_t *dst, uint32_t wb, uint32_t hb, uint32_t d, uint32_t bpb,
                         uint32_t bh_log2, uint32_t bd_log2, bool to_linear) {
    uint32_t row_bytes = wb * bpb;
    uint32_t width_gobs = (row_bytes + 63) / 64;
    uint32_t gob_rows = 8u << bh_log2;
    uint64_t blocks_rows = (hb + gob_rows - 1) / gob_rows;
    for (uint32_t z = 0; z < d; z++) {
        for (uint32_t y = 0; y < hb; y++) {
            uint8_t *lin = dst + ((uint64_t)z * hb + y) * row_bytes;
            const uint8_t *lin_src = src + ((uint64_t)z * hb + y) * row_bytes;
            // 16-byte runs are contiguous in both layouts
            for (uint32_t x = 0; x < row_bytes; x += 16) {
                uint32_t n = row_bytes - x < 16 ? row_bytes - x : 16;
                uint64_t o = gob_offset(x, y, z, width_gobs, bh_log2, bd_log2, blocks_rows);
                if (to_linear) memcpy(lin + x, src + o, n);
                else memcpy(dst + o, lin_src + x, n);
            }
        }
    }
}

// A GOB is 8 rows of 64 bytes, each row four 16-byte pieces at fixed offsets in it (gob_offset's within-GOB part: row
// r, piece p at ((r >> 1) << 6) | ((r & 1) << 4) | (p & 1) << 5 | (p >> 1) << 8), so a whole GOB column is read from its
// 512 contiguous bytes and written as 8 rows of 64: what is the same for the whole GOB is worked out once, and the
// copies are constant-size. (Row by row, the same GOB column of a row sits one block apart, a multiple of the cache's
// 4 KB set period: every piece evicted the one before it.) Only the partial column at the right edge (a row not a
// multiple of 64 bytes) goes piece by piece.
void tegra_deswizzle(const uint8_t *src, uint8_t *dst, uint32_t wb, uint32_t hb, uint32_t d, uint32_t bpb,
                     uint32_t bh_log2, uint32_t bd_log2) {
    uint32_t row_bytes = wb * bpb;
    uint32_t width_gobs = (row_bytes + 63) / 64, full_gobs = row_bytes / 64;
    uint32_t gob_rows = 8u << bh_log2;
    uint64_t blocks_rows = (hb + gob_rows - 1) / gob_rows;
    uint64_t block_size = 512ull << (bh_log2 + bd_log2);
    for (uint32_t z = 0; z < d; z++) {
        uint64_t z_base = (uint64_t)(z >> bd_log2) * blocks_rows, z_in_block = z & ((1u << bd_log2) - 1);
        for (uint32_t y0 = 0; y0 < hb; y0 += 8) {
            uint32_t gob_y = y0 >> 3, rows = hb - y0 < 8 ? hb - y0 : 8;
            const uint8_t *g = src + ((z_base + (gob_y >> bh_log2)) * width_gobs * block_size) +
                               ((z_in_block << bh_log2 | (gob_y & ((1u << bh_log2) - 1))) * 512);
            uint8_t *lin = dst + ((uint64_t)z * hb + y0) * row_bytes;
            for (uint32_t gx = 0; gx < full_gobs; gx++, g += block_size, lin += 64) {
                for (uint32_t r = 0; r < rows; r++) {
                    const uint8_t *p = g + ((r >> 1) << 6) + ((r & 1) << 4);
                    uint8_t *o = lin + (uint64_t)r * row_bytes;
                    memcpy(o, p, 16);
                    memcpy(o + 16, p + 32, 16);
                    memcpy(o + 32, p + 256, 16);
                    memcpy(o + 48, p + 288, 16);
                }
            }
            if (full_gobs * 64 < row_bytes)
                for (uint32_t r = 0; r < rows; r++)
                    for (uint32_t x = full_gobs * 64; x < row_bytes; x += 16) {
                        uint32_t n = row_bytes - x < 16 ? row_bytes - x : 16;
                        memcpy(dst + ((uint64_t)z * hb + y0 + r) * row_bytes + x,
                               src + gob_offset(x, y0 + r, z, width_gobs, bh_log2, bd_log2, blocks_rows), n);
                    }
        }
    }
}

void tegra_swizzle(const uint8_t *src, uint8_t *dst, uint32_t wb, uint32_t hb, uint32_t d, uint32_t bpb,
                   uint32_t bh_log2, uint32_t bd_log2) {
    copy_surface(src, dst, wb, hb, d, bpb, bh_log2, bd_log2, false);
}
