# wlsnap AUR Package

This directory contains the files needed to build and install wlsnap from the Arch User Repository (AUR).

## Files

| File | Description |
|------|-------------|
| `PKGBUILD` | Build script for makepkg |
| `.SRCINFO` | AUR metadata (auto-generated from PKGBUILD) |
| `wlsnap.install` | Post-install message |

## Manual Build

```bash
cd aur
makepkg -si
```

## Update .SRCINFO

After modifying `PKGBUILD`, regenerate `.SRCINFO`:

```bash
cd aur
makepkg --printsrcinfo > .SRCINFO
```

## AUR Submission

To submit to AUR:

1. Create an account at https://aur.archlinux.org/
2. Upload the SSH public key in your account settings
3. Clone the AUR repository:
   ```bash
   git clone ssh://aur@aur.archlinux.org/wlsnap.git
   ```
4. Copy `PKGBUILD`, `.SRCINFO`, and `wlsnap.install` into the cloned repo
5. Commit and push:
   ```bash
   git add PKGBUILD .SRCINFO wlsnap.install
   git commit -m "Initial release v0.1.0"
   git push origin master
   ```

## Version Bump

When a new version is released:

1. Update `pkgver` in `PKGBUILD`
2. Reset `pkgrel=1`
3. Update `sha256sums` with the new tarball hash
4. Regenerate `.SRCINFO`
5. Commit and push to AUR
