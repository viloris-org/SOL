# sol-logind

SOL's login screen service - a macOS-inspired authentication interface that displays before the user session starts.

## Architecture

`sol-logind` is a system service that:
- Runs before the compositor and shell
- Displays a visual login interface using SolKit components
- Handles user selection and password authentication
- Spawns the user session after successful login

## Position in the architecture

```
Boot
  ↓
sol-init starts sol-logind
  ↓
sol-logind displays login UI
  ↓
User authenticates
  ↓
sol-logind spawns compositor + shell
  ↓
User session running
```

## Status

**Phase 1**: Visual-only implementation with authentication stub.
- macOS-like UI with user avatars and password field
- Uses sol-design tokens for consistent styling
- Authentication stub (always succeeds for development)

**Future phases**:
- Phase 2: Real PAM authentication integration
- Phase 3: Full session management and user switching
- Phase 4: Biometric auth, accessibility features

## UI Features

- **User avatar grid**: Circular avatars for account selection
- **Password field**: With show/hide toggle
- **Primary action button**: "Log In" with accent styling
- **System actions**: Sleep/Shutdown buttons (future)
- **Material design**: Frosted glass panel effect using `Material::Floating`

## Running

```bash
# Development mode (standalone)
cargo run -p sol-logind

# With compositor integration (Phase 2)
# Will be started automatically by sol-init
```

## Design

The login screen follows SOL's design language:
- Uses `Material::Floating` for the central panel
- `Color::Accent` for primary actions
- `Spacing::Xl` for generous macOS-like padding
- `Motion::Window` for entrance animations
- All colors and metrics from sol-design tokens

## Security

Phase 1 uses an authentication stub for development. Future phases will:
- Integrate with PAM for real authentication
- Rate-limit failed attempts
- Properly secure session tokens
- Avoid user enumeration vulnerabilities

## See also

- [Services README](../README.md)
- [PRD §11 Shell](../../docs/PRD.md)
- [Architecture](../../docs/architecture.md)
