# Rei theme — design

*Date: 2026-10-23. Personal theme in the user's Darwan fork; mainly for the
SDDM login screen, usable as a lock screen too.*

## Intent

The user runs a **fork** of Darwan (`~/Scripts/Others/darwan`, remote
`RushiVivek/darwan`, packaged `darwan 0.6.0-1` on the machine); its themes
come from qylock. Rei is **primarily the login screen (SDDM)**; the lock
screen stays on `nier-automata` for now. The user
loved **Silent SDDM's Rei look** (`/usr/share/sddm/themes/silent`,
`configs/rei.conf`, background `backgrounds/rei.mp4`), and what they like
in it is specifically **the background video and the blur**: the screen
rests blurred with just the time, and the moment you interact, the blur
melts away and the password field appears.

Two earlier experiments in the fork, `rei-waterfall` and `rei-last-of-us`,
put the Rei video behind other themes' layouts. They look fine but have no
blur. This is a **new, third theme**, built for Rei; the UI need not copy
Silent.

## Goals

- A new Darwan theme, id **`rei`**, display name **"Rei"**, in the fork's
  `themes/rei/`, following `docs/theme-contract.md` and passing
  `just test lint check` (and `check --no-fonts`).
- **Two stages:**
  - **Resting:** the Rei video, blurred and slightly darkened; only a big
    clock and the date, in the middle of the left side.
  - **Awake:** on the first key or click, the blur animates away (sharp
    video), the clock moves to its awake spot, and the login (welcome label,
    user name, password field, unlock action) fades in. The first key typed
    while resting is already in the password field: the field is focused
    and only hidden by opacity, in both stages.
- **Two layouts, chosen by a setting** (user couldn't pick; wants both):
  - **Rise:** awake, the clock shrinks up to the top-left; login mid-left;
    power/session row bottom-left.
  - **Corners:** awake, the clock shrinks down to the bottom-left; login
    bottom-right (right-aligned); power/session row top-right.
  - Both rest identically (big clock, mid-left).
- **Blur strength as a setting**, so the user can tune it later. Default
  matches Silent's Rei: Silent applies `MultiEffect { blur: 1.0; blurMax:
  32 }` on the lock stage and 0 on the login stage, animating over 400 ms.
- **Font: Outfit Black** (from `rei-last-of-us/font/Outfit-Black.ttf`,
  already in the fork) for clock and text, user-overridable through the
  kit's font roles.
- **Colours: Silent's lavender `#C3C6FB`** as the text/accent; a muted
  variant for labels. User-overridable through the kit.

## Settings (darwan.toml `[[option]]`, defaults in theme.conf)

| key | type | values | default |
|---|---|---|---|
| `layout` | enum | `rise` "Rise", `corners` "Corners" | `rise` |
| `blur` | int | 0–64; 0 = no blur | 32 |

`[supports]`: `clock_format`, `date_format`, `background`, `colors`,
`fonts = ["text", "clock"]`, `motion`.

## Behaviour details (user-approved unless marked)

- Approved: blurred rest with clock; interaction unblurs and shows the
  password field; first keystroke goes into the field.
- *(Claude's call)* Awake → resting again after Darwan's screensaver timing:
  the kit's `Ambient` (`saver.wake`) is reused as the stage — ambient =
  resting, revealed = awake. On `darwan lock` (no saver) and SDDM, the
  theme holds its own `awake` flag: false at start, true on first
  key/click, back to false after 30 s idle with an empty field.
  Confirmed in the repo: `darwan lock` starts with `ambient = false` and
  sets it true itself after `saver.return_after` (30 s) idle with an empty
  field (`runtime/lock_shell.qml`); `darwan saver` starts ambient; SDDM
  never sets it; `just check` sets `host.ambient` and unlocks through the
  input gate (`runtime/check_shell.qml`). So: resting = `saver.active` or
  the theme's own `awake` false (start of `darwan lock` and SDDM); awake on
  first key/click.
- Approved: **power and reboot buttons on both the lock screen and SDDM**
  (`sddm.powerOff()` / `sddm.reboot()`, shown when `canPowerOff` /
  `canReboot`); the **session picker only under SDDM**
  (`sddm.hostName !== undefined`).
- *(Claude's call)* Wrong password: shake the login block and show a short
  message, as `rei-last-of-us` does; field clears.
- *(Claude's call)* Readability over the sharp video: a soft dark gradient
  behind the login side (left for Rise, bottom for Corners), fading in with
  the awake stage.
- *(Claude's call)* Reduce motion: stages switch instantly (`kit.dur`);
  the blur still turns on/off.
- The user's own background (kit `Background`) replaces the video and is
  blurred the same way; the theme's video is not decoded while it shows.

## Non-goals

- On-screen keyboard, keyboard-layout picker (never used).
- A 1:1 copy of Silent's UI.
- Changing `rei-waterfall` / `rei-last-of-us`.
- Upstreaming to Darwan (the user can decide later).
- Installing the fork or applying the theme to SDDM / the lock screen: the
  user reinstalls Darwan and applies it themselves. The work ends at the
  fork's repo, tested with `just` from the checkout.
- Any mention in Vesta: Vesta only says Darwan is its lock screen.

## Anti-goals

- A keystroke lost while resting, or a stage you must click before typing.
- The blur costing noticeable GPU while awake: when blur reaches 0 the
  effect must stop processing (e.g. `blurEnabled` false / layer off).
- Animating `blurMax` per frame (MultiEffect rebuilds its shader when
  `blurMax` changes); animate `blur` 1→0 with `blurMax` = the setting.
- Any QML warning (fails `just check`).

## Constraints

- Theme contract rules: guarded fonts (`FolderListModel` + `FontLoader`),
  `showDirs: false`, no always-running Timer under 100 ms, videos through
  `MediaPlayer` + `VideoOutput`, every `[supports]` flag backed, kit files
  identical to `runtime/theme-kit` (`just theme-kit rei`).
- Assets: `bg.mp4` = Silent's lavender-tinted Rei video, the same 2.7 MB
  1080p25 file the two experiments ship (chosen over `rei.bright.mp4`: 4K60,
  62 MB, full colour, brighter left side, a watermark bottom-right; the user
  can still pick it through Darwan's background setting); `preview.png` 1280×720 via
  `darwan preview --shot`; `preview.webp` optional.
- `crates/darwan-core/tests/themes.rs` hard-codes the theme count (42 →
  43).
- Blurring the video works in every host: Darwan's `MpvVideo` is
  capturable (`forest` already blurs a video); `MultiEffect` under SDDM.
- `darwan set lock.theme <id>` rejects ids that aren't installed
  (`crates/darwan-core/src/settings.rs`, `unknown_theme`); the installed
  package (`/usr/share/darwan/themes`) has the two Rei experiments but not
  `rei`. The current lock theme is `nier-automata`.
- Testing: the fork is run from the checkout with `just preview rei` /
  `just preview rei --saver` / `just check`; nothing is installed.
- SDDM is the main host: under SDDM `sddm.hostName` is set, so the
  session picker shows too, and the theme starts resting (blurred
  clock) until the first key or click.

## Left to the builder

Exact sizes, spacing, easing; the date format default; what the welcome
label says; how the two layouts share code inside `Main.qml`.
