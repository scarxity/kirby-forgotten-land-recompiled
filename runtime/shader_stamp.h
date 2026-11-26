// What the prepared shaders were made for, as one text: the runtime writes it when a full shader preparation ends
// (runtime/nvn/shader_prep.c) and checks it at every play, whichever launcher started the game (runtime/main.c). The
// Tauri launcher asks the runtime (--shaders-status) instead of keeping a rule of its own.
//
// One line per field, "<name> <value>". A field that differs from what this run expects means the shaders must be
// prepared again; shader_stamp_diff names the first one.
#pragma once
#include <stdbool.h>
#include <stddef.h>

#define SHADER_STAMP_FILE "cache/shaders-prepared.txt"  // (in the data root: wiping cache/ wipes the claim with the cache)

typedef struct {
    const char *runtime;  // the app version, the shader translator and the cache format
    const char *exe;      // the runtime file's size and time
    const char *gpu;      // the GPU and its driver (the pipeline cache's own key)
    const char *list;     // the variant list the preparation read (a hash)
    const char *code;     // the game code library: its id, size and time
    const char *mods;     // the mods switched on, in order
} ShaderStampIn;

void shader_stamp_format(char *out, size_t n, const ShaderStampIn *in);
// true when have is not the stamp want describes; field gets the name of the first field that differs ("none": no stamp)
bool shader_stamp_diff(const char *have, const char *want, char *field, size_t n);
bool shader_stamp_read(const char *path, char *out, size_t n);
bool shader_stamp_write(const char *path, const char *text);  // (temp file + rename: never half-written)
