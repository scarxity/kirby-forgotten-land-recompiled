#include "shader_stamp.h"

#include <stdio.h>
#include <string.h>

void shader_stamp_format(char *out, size_t n, const ShaderStampIn *in) {
    snprintf(out, n, "shaders-prepared 1\nruntime %s\nexe %s\ngpu %s\nlist %s\ncode %s\nmods %s\n", in->runtime, in->exe, in->gpu,
             in->list, in->code, in->mods);
}

// The line of text starting with name and a space; its length (without the newline), or -1
static long field_line(const char *text, const char *name, size_t name_len, const char **at) {
    for (const char *l = text; *l;) {
        const char *end = strchr(l, '\n');
        size_t len = end ? (size_t)(end - l) : strlen(l);
        if (len > name_len && !strncmp(l, name, name_len) && l[name_len] == ' ') return *at = l, (long)len;
        l += len + (end != NULL);
    }
    return -1;
}

bool shader_stamp_diff(const char *have, const char *want, char *field, size_t n) {
    if (!have || !*have) return snprintf(field, n, "none"), true;
    for (const char *l = want; *l;) {
        const char *end = strchr(l, '\n');
        size_t len = end ? (size_t)(end - l) : strlen(l), name_len = strcspn(l, " ");
        const char *got = NULL;
        long got_len = field_line(have, l, name_len, &got);
        if (got_len != (long)len || strncmp(got, l, len)) return snprintf(field, n, "%.*s", (int)name_len, l), true;
        l += len + (end != NULL);
    }
    return false;
}

bool shader_stamp_read(const char *path, char *out, size_t n) {
    FILE *f = fopen(path, "rb");
    if (!f) return false;
    size_t got = fread(out, 1, n - 1, f);
    fclose(f);
    out[got] = 0;
    return true;
}

bool shader_stamp_write(const char *path, const char *text) {
    char tmp[1100];
    snprintf(tmp, sizeof tmp, "%s.tmp", path);
    FILE *f = fopen(tmp, "wb");
    if (!f) return false;
    bool ok = fwrite(text, 1, strlen(text), f) == strlen(text);
    ok &= fclose(f) == 0;
    if (ok) {
#ifdef _WIN32
        remove(path);  // (rename doesn't replace on Windows; a crash here leaves no stamp, which prepares again)
#endif
        ok = rename(tmp, path) == 0;
    }
    if (!ok) remove(tmp);
    return ok;
}
