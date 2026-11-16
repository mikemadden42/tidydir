# tidydir
Tidy up home directory

Moves each regular file in a directory (your home directory by default) into
`Documents/<extension>/` inside that directory. Extensions are lowercased, so
`a.txt` and `B.TXT` both land in `Documents/txt/`.

Left alone: dot files, files without an extension, `*.9` files, symlinks and
directories. Existing files are never overwritten.

```sh
tidydir --dry-run      # preview what would move in your home directory
tidydir                # tidy your home directory
tidydir ~/Downloads    # tidy another directory
```

Exits with status 1 if any file could not be moved, 2 on bad arguments.

MSRV: 1.58.1
