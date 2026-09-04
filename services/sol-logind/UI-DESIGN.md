# macOS-like Login Screen UI Layout

## Visual Design

```
┌───────────────────────────────────────────────────────────────┐
│                                                               │
│                                                               │
│                    [Wallpaper Background]                     │
│                    Color::Surface from tokens                 │
│                                                               │
│                                                               │
│              ╔════════════════════════════╗                   │
│              ║                            ║                   │
│              ║    ╭────╮ ╭────╮ ╭────╮   ║  Avatar Grid     │
│              ║    │ 👤 │ │ 👤 │ │ 👤 │   ║  Radius::Full    │
│              ║    ╰────╯ ╰────╯ ╰────╯   ║  (circular)      │
│              ║      John   Jane   Admin   ║                   │
│              ║                            ║                   │
│              ║      John Appleseed        ║  Display Name    │
│              ║     FontStyle::Title       ║                   │
│              ║                            ║                   │
│              ║  ┌──────────────────────┐ ║  Password Field  │
│              ║  │ ••••••••••      👁  │ ║  with toggle     │
│              ║  └──────────────────────┘ ║  Radius::Sm      │
│              ║                            ║                   │
│              ║     ┌──────────────┐      ║  Primary Button  │
│              ║     │   Log In     │      ║  Color::Accent   │
│              ║     └──────────────┘      ║  Radius::Sm      │
│              ║                            ║                   │
│              ╚════════════════════════════╝  Panel           │
│                                               Color::Elevated │
│                                               Radius::Md      │
│                                                               │
│  [Sleep]                            [Shutdown]   (Future)    │
│                                                               │
└───────────────────────────────────────────────────────────────┘
```

## Design Token Usage

### Colors
- **Background**: `Color::Surface` - Base window surface
- **Panel**: `Color::Elevated` - Raised login panel
- **Text (primary)**: `Color::TextPrimary` - User name
- **Text (secondary)**: `Color::TextSecondary` - Placeholders
- **Accent**: `Color::Accent` - Login button background
- **Border**: `Color::Border` - Input field borders

### Spacing
- **Small**: `Spacing::Sm` (8px) - Between text lines
- **Medium**: `Spacing::Md` (16px) - Between form elements
- **Large**: `Spacing::Lg` (24px) - Section separation
- **XLarge**: `Spacing::Xl` (32px) - Panel padding

### Radius
- **Full**: `Radius::Full` - Avatar circles (100% rounded)
- **Medium**: `Radius::Md` - Panel corners (12px)
- **Small**: `Radius::Sm` - Input fields and buttons (6px)

### Typography
- **Display**: `FontStyle::Display` - (Future: SOL logo/brand)
- **Title**: `FontStyle::Title` - User display name
- **Body**: `FontStyle::Body` - Helper text
- **Label**: `FontStyle::Label` - Button labels

## State Flow

```
┌──────────────────┐
│ SelectingUser    │  Initial state
│                  │  - Show avatar grid
│                  │  - Highlight selected user
└────────┬─────────┘
         │ User clicks avatar
         ▼
┌──────────────────┐
│ EnteringPassword │  Password entry
│                  │  - Show password field
│                  │  - Enable/disable login button
└────────┬─────────┘
         │ User clicks "Log In"
         ▼
┌──────────────────┐
│ Authenticating   │  Loading state
│                  │  - Show spinner (Phase 2)
│                  │  - Disable all inputs
└────────┬─────────┘
         │ Auth succeeds
         ▼
┌──────────────────┐
│ Authenticated    │  Success
│                  │  - Fade out panel
│                  │  - Spawn session
└──────────────────┘
```

## Component Hierarchy

```
LoginFrame
├── Background (Color::Surface)
├── Panel (Color::Elevated)
│   ├── AvatarGrid
│   │   ├── Avatar (circular, Radius::Full)
│   │   ├── Avatar
│   │   └── Avatar
│   ├── UserName (FontStyle::Title)
│   ├── PasswordField
│   │   ├── Input (masked/visible)
│   │   └── ToggleButton (eye icon)
│   └── LoginButton (Color::Accent)
└── SystemActions (future)
    ├── SleepButton
    └── ShutdownButton
```

## Interaction States

### Avatar Selection
- **Normal**: Gray border, 50% opacity
- **Hovered**: Scale 1.05, border visible
- **Selected**: Color::Accent border, 100% opacity

### Password Field
- **Empty**: Placeholder "Password"
- **Hidden**: Dots (••••••••)
- **Visible**: Plain text
- **Focused**: Border Color::Accent

### Login Button
- **Disabled**: Opacity 50%, not clickable (empty password)
- **Normal**: Color::Accent background
- **Hovered**: Slightly brighter
- **Pressed**: Slightly darker
- **Loading**: Spinner animation (Phase 2)

## Animations (Phase 3)

### Panel Entrance (Motion::Window)
```
Opacity: 0 → 1 (fade in)
Scale: 0.95 → 1.0 (gentle zoom)
Duration: 300ms
Easing: ease-out
```

### Avatar Selection
```
Scale: 1.0 → 1.05 (on hover)
Duration: 150ms
Easing: ease-in-out
```

### Password Visibility Toggle
```
Opacity: 0 → 1 (crossfade)
Duration: 100ms
```

### Login Success
```
Keep the session lock engaged while sol-session starts
Wait for the shell's first committed SCP surface
Content opacity: 1 → 0 (0–160ms)
Panel material opacity: 1 → 0 (80–260ms)
Easing: cubic-bezier(0.23, 1, 0.32, 1)
Release the lock only after the final frame is presented

Reduced motion:
Content + material opacity: 1 → 0
Duration: 160ms; no spatial transform or spring
```

## Accessibility

### Keyboard Navigation
- `Tab`: Next element
- `Shift+Tab`: Previous element
- `Arrow Left/Right`: Select user (in avatar grid)
- `Enter`: Submit login (when enabled)
- `Escape`: Clear password field
- `Space`: Toggle password visibility (when focused)

### Screen Reader
- Avatar grid: "3 user accounts available"
- Selected user: "John Appleseed, selected"
- Password field: "Password, secure text field"
- Visibility toggle: "Show password" / "Hide password"
- Login button: "Log in, button, enabled/disabled"

### High Contrast Mode
- Increase border thickness
- Use solid colors (no translucency)
- Higher contrast text

## Responsive Behavior (Future)

### Large Screens (>1920px)
- Larger avatar images (96px)
- Increased panel size
- More generous spacing

### Small Screens (<1366px)
- Smaller avatars (64px)
- Compact spacing
- Vertical layout if needed

### HiDPI/Retina
- All metrics scale with DPI
- Avatar images use @2x assets
- Crisp rendering at any scale

## Panel Specifications

```rust
Color::Elevated (solid white/light gray)
- Clean, professional appearance
- Clear contrast with background
- Maintains visual hierarchy
```

## Implementation Notes

**Phase 1** (Current):
- UI state machine complete
- All logic tested (23 tests passing)
- Design tokens integrated
- Renderer-neutral frame generation

**Phase 2** (Visual Rendering):
- Slint adapter for actual rendering
- Avatar image loading from disk
- Password field with eye icon

**Phase 3** (Polish):
- Entrance animations
- Hover effects
- Loading states
- Error messages

**Phase 4** (System Integration):
- PAM authentication
- Session spawning
- Environment setup
- Fast user switching
