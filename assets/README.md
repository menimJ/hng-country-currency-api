# assets/

This folder holds static files needed by the app. The API’s summary image
generator uses a TrueType font to render text.

## Why a TTF?
`imageproc` + `rusttype` require a real `.ttf` file to draw text. The repo ships
a tiny placeholder so builds don’t fail, but image rendering will **skip/fail**
until you replace it with a real font.

## Options

### 1) Embed at compile time (current code)
The code uses:
```rust
let font_data: &[u8] = include_bytes!("../../assets/DejaVuSans.ttf");
