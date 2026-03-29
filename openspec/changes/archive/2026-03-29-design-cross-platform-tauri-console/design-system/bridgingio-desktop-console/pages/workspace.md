# Workspace Page Overrides

> **PROJECT:** BridgingIO Desktop Console
> **Generated:** 2026-03-29 16:40:34
> **Page Type:** Dashboard / Data View

> ⚠️ **IMPORTANT:** Rules in this file **override** the Master file (`design-system/MASTER.md`).
> Only deviations from the Master are documented here. For all other rules, refer to the Master.

---

## Page-Specific Rules

### Layout Overrides

- **Max Width:** 1200px (standard)
- **Layout:** Full-width sections, centered content
- **Sections:** 1. Hero headline, 2. Short description, 3. Benefit bullets (3 max), 4. CTA, 5. Footer

### Spacing Overrides

- No overrides — use Master spacing

### Typography Overrides

- No overrides — use Master typography

### Color Overrides

- **Strategy:** Minimalist: Brand + white #FFFFFF + accent. Buttons: High contrast 7:1+. Text: Black/Dark grey

### Component Overrides

- Avoid: Leave UI frozen with no feedback
- Avoid: Use arbitrary large z-index values
- Avoid: Jump directly without transition

---

## Page-Specific Components

- No unique components for this page

---

## Recommendations

- Effects: Deal movement animations, metric updates, leaderboard ranking changes, gauge needle movements, status change highlights
- Animation: Use skeleton screens or spinners
- Layout: Define z-index scale system (10 20 30 50)
- Navigation: Use scroll-behavior: smooth on html element
- CTA Placement: Center, large CTA button
