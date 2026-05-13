# whispr

Whispr Flow clone for macOS. Hold **Fn**, speak, release — cleaned dictation lands in the focused app.

## Stack

- Rust + [gpui](https://github.com/zed-industries/zed) + [gpui-component](https://longbridge.github.io/gpui-component/)
- OpenAI `gpt-4o-transcribe` (STT) + `gpt-4o-mini` (cleanup)
- `cpal` for capture, `CGEventTap` for the Fn key, `arboard` / `enigo` for output

## Run

```bash
cp .env.example .env
# put your OpenAI key in .env
just run
```

On first launch macOS will ask for **Microphone** and **Accessibility** permission. Both required.

## Settings

Menubar → Settings. API key, output mode (paste vs type), cleanup prompt, custom vocabulary. Persisted to `~/.config/whispr/config.toml`.

## Commands

```
just         # run release build
just dev     # run debug with verbose logs
just check
just lint
just fmt
```
