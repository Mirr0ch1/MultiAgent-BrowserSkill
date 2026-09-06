# MultiAgent-BrowserSkill — Install Guide for AI Agents

You are an AI agent setting up MultiAgent-BrowserSkill for the user. Done =
`bsk doctor` reports every check `ok` (or `na`). Never use `sudo`; you cannot
install the extension yourself (Step 3 is the user's job).

This fork is **not** distributed via the Chrome Web Store / Edge Add-ons, and
does not ship prebuilt release binaries yet — install from source.

## 1. Build the CLI and install the skill

Requires Rust (Cargo) ≥ 1.98.

```bash
git clone https://github.com/Mirr0ch1/MultiAgent-BrowserSkill.git
cd MultiAgent-BrowserSkill
cargo build --release -p bsk
export PATH="$PWD/target/release:$PATH"
bsk install-skill --yes
```

On Windows, add `target\release` to `PATH` and run the same commands in a
terminal with Rust installed.

## 2. Run `bsk doctor`

```bash
bsk doctor
```

Each `fail` row prints a `hint` — follow it and re-run once. A fresh install
where only `extension connected` fails is expected; go to Step 3.

## 3. Load the browser extension

If `extension connected` is `FAIL` (`0 browsers connected`), the user likely
has not loaded the browser extension yet. Build it first (requires Node.js +
pnpm):

```bash
cd apps/extension
pnpm install
pnpm build
```

Then ask the user to:

> Open `chrome://extensions` (or `edge://extensions`), enable **Developer
> mode**, click **Load unpacked**, and select
> `apps/extension/.output/chrome-mv3` in the repo. Open the popup and wait
> until it turns green. Reply when done.

Then run `bsk doctor` once more. All `ok`/`na` → tell the user it's ready.

## Gateway mode (multi-agent, multi-host)

See the **Gateway mode** section in [`README.md`](README.md) for starting the
daemon with `--gateway`, `--lan-cidr`, `--agent-token` / `--extension-token`,
and connecting remote agents via `bsk --host … --port … --agent-token …`.
