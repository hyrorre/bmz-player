# Synthetic DXA fixture

`font-v3.dxa` is generated for BMZ tests, not taken from a third-party skin.
It uses DXArchive format version 3 and the default 12-byte key.

- `font.lr2font`: size 1, margin 0, page `page.bmp`, ASCII A at (0,0,1,1).
  Stored with the DXArchive LZ header and literal-only commands.
- `page.bmp`: 1x1 red pixel, uncompressed 24-bit BMP with a 40-byte DIB header.

The index contains two file records (44 bytes each) and a root directory record
(16 bytes). Filenames include padded uppercase and original-case strings.
This exercises archive lookup, decompression, font parsing, image decode and caches
without external assets or extraction.
