import os, sys, zipfile

src, out = sys.argv[1], sys.argv[2]
parent = os.path.dirname(os.path.abspath(src))
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for root, _, files in os.walk(src):
        for f in files:
            p = os.path.join(root, f)
            z.write(p, os.path.relpath(p, parent).replace(os.sep, "/"))
