/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    container: {
      center: true,
      padding: "2rem",
      screens: { "2xl": "1400px" },
    },
    extend: {
      fontFamily: {
        // Default UI font. Tailwind's `font-sans` utility resolves
        // here. Geist Variable provides every weight (100-900) from
        // one file. Fallbacks: `system-ui` is the OS's default UI
        // font (Segoe UI on Windows, SF on macOS); `sans-serif` is
        // the catch-all generic family — both unquoted, mandatorily.
        sans: ['Geist', 'system-ui', '-apple-system', 'sans-serif'],
        // Default mono font. Tailwind's `font-mono` utility resolves
        // here. `ui-monospace` is the OS's default mono.
        mono: ['"Geist Mono"', 'ui-monospace', 'Menlo', 'Consolas', 'monospace'],
        // Pixel display variants — used as utility classes
        // (`font-pixel-square`, `font-pixel-grid`, etc.). Each
        // pixel face has its own `font-family` because they're
        // five distinct typefaces, not weights of one.
        'pixel-square':   ['"Geist Pixel Square"', 'monospace'],
        'pixel-grid':     ['"Geist Pixel Grid"', 'monospace'],
        'pixel-circle':   ['"Geist Pixel Circle"', 'monospace'],
        'pixel-triangle': ['"Geist Pixel Triangle"', 'monospace'],
        'pixel-line':     ['"Geist Pixel Line"', 'monospace'],
      },
      colors: {
        // shadcn-style CSS variables — defined in index.css. The dark
        // values map to NeuroGrim Design System tokens via the bridge
        // there (see `:root + .dark` blocks). Light mode keeps the
        // shadcn defaults until a dedicated light bridge is authored.
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        card: {
          DEFAULT: "hsl(var(--card))",
          foreground: "hsl(var(--card-foreground))",
        },
        popover: {
          DEFAULT: "hsl(var(--popover))",
          foreground: "hsl(var(--popover-foreground))",
        },
        // ── NeuroGrim Design System extension ─────────────────────────
        // Direct access to the foundation tokens for non-shadcn
        // surfaces. Use these when the intent is "pick this NeuroGrim
        // surface tier" rather than "shadcn role" — e.g., a dashboard
        // table row tinted with `bg-ng-bg-elevated` instead of
        // `bg-secondary`. Both resolve to the same color in dark mode
        // through the bridge; the `ng-*` form preserves the design-
        // system intent at the call site. Source of truth:
        // `…/children/neurogrim-ide/design/dashboard/tailwind-extension.md`
        "ng-bg-base":     "hsl(var(--bg-base))",
        "ng-bg-elevated": "hsl(var(--bg-elevated))",
        "ng-bg-surface":  "hsl(var(--bg-surface))",
        "ng-bg-overlay":  "hsl(var(--bg-overlay))",
        "ng-bg-active":   "hsl(var(--bg-active))",
        "ng-text-primary":   "hsl(var(--text-primary))",
        "ng-text-secondary": "hsl(var(--text-secondary))",
        "ng-text-muted":     "hsl(var(--text-muted))",
        "ng-border-subtle":  "hsl(var(--border-subtle))",
        "ng-border-default": "hsl(var(--border-default))",
        "ng-border-strong":  "hsl(var(--border-strong))",
        "ng-accent": {
          blue:   "hsl(var(--accent-blue))",
          teal:   "hsl(var(--accent-teal))",
          green:  "hsl(var(--accent-green))",
          amber:  "hsl(var(--accent-amber))",
          red:    "hsl(var(--accent-red))",
          purple: "hsl(var(--accent-purple))",
        },
      },
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
        // NeuroGrim Design System radii
        "ng-sm":   "var(--r-sm)",
        "ng-md":   "var(--r-md)",
        "ng-lg":   "var(--r-lg)",
        "ng-xl":   "var(--r-xl)",
        "ng-pill": "var(--r-pill)",
      },
      transitionDuration: {
        "ng-instant": "var(--dur-instant)",
        "ng-fast":    "var(--dur-fast)",
        "ng-normal":  "var(--dur-normal)",
        "ng-slow":    "var(--dur-slow)",
        "ng-score":   "var(--dur-score)",
      },
      transitionTimingFunction: {
        "ng-out":    "var(--ease-out-fast)",
        "ng-in":     "var(--ease-in-fast)",
        "ng-spring": "var(--ease-spring)",
      },
    },
  },
  plugins: [require("tailwindcss-animate")],
};
