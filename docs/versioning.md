# SOL Versioning

## Current Version

**0.1.0-dawn.1 "Polaris"**

> 黎明 (Dawn) - The first light before sunrise  
> 北极星 (Polaris) - The guiding star

## Version Files

- `VERSION` - Semantic version with prerelease tag (e.g., `0.1.0-dawn.1`)
- `CODENAME` - Release codename (e.g., `Polaris`)

These files are tracked in git and used by build scripts to generate:
- ISO filenames: `sol-0.1.0-dawn.1-x86_64.iso`
- GRUB boot menu entries
- System identification: `/etc/sol-release`
- GitHub releases

## Version Scheme

SOL uses **Semantic Versioning** (SemVer) with codenames:

```
MAJOR.MINOR.PATCH "CODENAME"
```

### Version 0.x - "Polaris" Era (Development)

| Version | Prerelease | Phase | Milestone |
|---------|------------|-------|-----------|
| 0.1.0-dawn.1 | Dawn  | Phase 0 | First ISO, foundation |
| 0.1.0-dawn.2 | Dawn | Phase 0-1 | SCP compositor, iteration |
| 0.1.0-sunrise.1 | Sunrise  | Phase 1 | Native renderer, input |
| 0.2.0-dawn.1 | Dawn | Phase 2 | SolKit framework |
| 0.2.0 | Stable | Phase 2 | First stable 0.2 |
| 0.5.0 | Stable | Phase 5 | System services complete |
| 0.9.0 | Stable | Phase 6 | Pre-1.0 candidate |

### Version 1.0+ - Future Stable Releases

Stable release numbering and codenames will be determined when 0.x series matures.

**Considerations for 1.0**:
- Requires completion of Phases 7-9 foundations
- Image-based updates and boot system stable
- Application security model complete
- SDK stability guarantees established
- Production-ready compositor and shell

Codename and versioning strategy for 1.x series will be decided closer to that milestone.

## Codename Themes

**Polaris Era (0.x)**: Foundation and navigation
- Polaris  - The North Star that guides all development

**Prerelease Tags**: Solar cycle themed
- `dawn`  - Initial development, first light
- `sunrise`  - Feature complete, rising
- `daylight` - Stable, full brightness (or use stable version)
- `rc` - Traditional release candidate when needed

**Future major versions**: Astronomical phenomena or celestial objects aligned with SOL (sun)

## Updating Version

```bash
# Update version files
echo "0.1.0-dawn.2" > VERSION
echo "Polaris" > CODENAME

# Build with new version
./scripts/build-iso.sh

# Tag for release
git tag -a v0.1.0-dawn.2 -m "Release 0.1.0-dawn.2 Polaris"
git push origin v0.1.0-dawn.2
```

## Prerelease Progression

```bash
# Development iterations
0.1.0-dawn.1  →  0.1.0-dawn.2  →  0.1.0-dawn.3

# Major milestone (Phase 1 complete)
0.1.0-sunrise.1  →  0.1.0-sunrise.2

# Release candidate
0.1.0-rc.1

# Stable release
0.1.0
```

## Version in Code

Build scripts automatically read these files:

```bash
# In scripts
SOL_VERSION=$(cat VERSION)
SOL_CODENAME=$(cat CODENAME)
SOL_FULL_VERSION="${SOL_VERSION} (${SOL_CODENAME})"
```

System version is stored in `/etc/sol-release`:

```ini
SOL_VERSION=0.1.0-dawn.1
SOL_CODENAME=Polaris
SOL_KERNEL=6.12.5
SOL_BUILD_DATE=2025-01-27 12:00:00 UTC
```

## CI/CD Integration

GitHub Actions workflow reads VERSION and CODENAME to:
- Name ISO artifacts
- Generate release notes
- Create GitHub releases
- Tag builds

See `.github/workflows/build-iso.yml` for implementation.
