# Music Player

Plays fresh variations of the game's music as it composes them: any
piece at any seed, a queue, and the composer's own draws when the queue
is empty. Every instrument plays on GeneralUser GS, which ships with the
player; the sampled banks are a download of their own (below).

## Running it

**Windows** — unzip `music-player-<version>-windows-x86_64.zip` and run
`music-player.exe`. The program is not signed, so the first time
Windows may show *Windows protected your PC*: choose **More info**, then
**Run anyway**.

**macOS** (11 Big Sur or later, Apple Silicon or Intel) — open
`music-player-<version>-macos-universal.dmg` and drag **Music Player**
into **Applications**. The app is not signed by an Apple developer
account, so macOS refuses it the first time. To allow it, once:

1. Open **Music Player** from Applications. macOS says it cannot verify
   the app is free of malware: choose **Done**.
2. Open **System Settings → Privacy & Security**, scroll to
   **Security**, and next to *"Music Player" was blocked* choose
   **Open Anyway**. Confirm with your password, then choose **Open**.

On macOS 14 Sonoma or earlier, Control-click the app in Applications,
choose **Open**, then **Open** again, instead.

If macOS says the app *is damaged and can't be opened*, it is the same
refusal worded differently. Clear it in Terminal, then open the app as
usual:

```
xattr -dr com.apple.quarantine "/Applications/Music Player.app"
```

**Linux** (x86_64) — unpack `music-player-<version>-linux-x86_64.tar.gz`
and run `./music-player`. It needs ALSA (`libasound2`) and an X11 or
Wayland session.

## The sampled banks

The guitars, basses, drum kit, piano, organ, saxophone and harmonica
sound as the game's music does on the sampled banks: the `.7z` of the
latest [music-banks release](https://github.com/ThePuug/unnamed-hex-tile-mmo/releases?q=music-banks&expanded=true).
Drop it onto the player's window; it unpacks into your own data folder
(about 1.9 GB) and the next piece plays on them. Where dropping does not
work (Linux under Wayland), hover the banks note under the sheet: it
names the folder to unpack the archive into, then start the player
again.

## Licences

GeneralUser GS is by S. Christian Collins; its licence is in
`licenses/GeneralUser-LICENSE.txt`. The player's faces are IBM Plex Mono
and Cormorant Garamond, under the SIL Open Font License in `licenses/`.
The banks archive carries its own licences and credits.
