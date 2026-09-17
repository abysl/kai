# Table artwork

Audience: players choosing artwork and maintainers updating the curated selection.

Open **Settings → look** to choose felt or a playmat.

## Your own picture

Choose **Choose playmat picture…** and select a PNG, JPEG or WebP file up to
16 MiB and 24 million pixels. Kai makes a smaller JPEG copy, up to 2048 pixels
per side, without the original file's metadata. The original stays untouched.
Your copy is saved on your device or in this browser's local storage, outside
the shared content store. Replacing it replaces the saved copy.

While selected at a multiplayer table, the picture is sent directly to the
other players over an encrypted peer connection. It is never uploaded to the
shared content service or advertised through its asset index. Other players
can still save or screenshot a picture they receive; use artwork you have
permission to share. Selecting felt or another mat stops offering the picture.

Enable **Disable opponent playmat** to show felt on opponents' sides instead.
It also prevents new playmat downloads and cancels pending personal-picture
transfers. This preference is saved on your device. Legacy arbitrary image
links are no longer fetched; download an authorized image yourself and choose
the file instead.

## Curated artwork and credits

The three curated images
below were supplied by the maintainer with permission to use them in Kai.
Their original pixels and embedded signatures are preserved. Display names
describe the selection; they do not claim authorship.

| Selection | Supplied file | BLAKE3 content address |
|---|---|---|
| Violet portrait | `009D61B8-8056-4E33-B16E-DB9B725352B2.jpg` | `8094d4c6f18176c4a02ca12f121cff81f9c7348aa05e4bed0c1480df4514b4f2` |
| Moonlit duet | `DFCF155B-1312-4E1F-841D-658C72467E1C.jpg` | `57399207905ee6819eb8ce447eed8bb05c2740f234775b35c031ea3d04540518` |
| Snow Moon Ahri | `Snow_Moon_Ahri_Playmat_Low_Res.png` | `8b3e0e6aeb059a3731a3b58a1fdac0dfeaf328cf2f97840d63d6bc6e937a16ec` |

**Snow Moon Ahri** is by [Clya Lyren](https://clyalyren.com/).
**Violet portrait** and **Moonlit duet** are by
[bbi](https://x.com/totatso). These credits are also linked from the in-app
playmat picker. All three supplied images retain their embedded signatures.

Images are fetched at runtime rather than stored in Git or bundled into the
application. Kai's GPLv3 code license does not grant a license to reuse these
images. Artwork rights remain with their respective owners; obtain permission
before redistributing artwork outside its authorized use.

Card backs and card faces are separate runtime game content. The card-back
sources are Piltover Archive's Riftbound CDN and Scryfall's card-back service.
They are not part of this permission-approved playmat set or the GPL license.
