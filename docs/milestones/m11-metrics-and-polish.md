# M11: Metrics & Polish

Usage metrics dashboard, theme support, accessibility audit, and UI polish. The app
becomes pleasant to use and verified accessible.

**Depends on:** M10 (cloud engines, engine selection, API key management)

**Delivers:** Metrics dashboard with real usage data. Dark, light, and high-contrast
themes. Comprehensive accessibility audit confirms all UI surfaces pass.

---

## Phase 1: Metrics Collection

Instrument the dictation pipeline to collect usage statistics.

### Work Items

1. **Metrics data model**
   - Add `metrics_daily` table to SQLite schema (migration v3):
     - `id` INTEGER PRIMARY KEY
     - `date_local` TEXT (ISO 8601 date in local timezone, `UNIQUE`)
     - `dictation_count` INTEGER DEFAULT 0
     - `total_words` INTEGER DEFAULT 0
     - `total_duration_ms` INTEGER DEFAULT 0
     - `total_wpm_weighted_sum` REAL DEFAULT 0.0 (sum of word_count_i * wpm_i)
   - Add `metrics_daily_engine` table (normalized, not JSON blob):
     - `id` INTEGER PRIMARY KEY
     - `date_local` TEXT (FK to `metrics_daily.date_local`)
     - `engine_id` TEXT (e.g., "whisper-tiny", "groq")
     - `dictation_count` INTEGER DEFAULT 0
     - `total_words` INTEGER DEFAULT 0
     - `total_duration_ms` INTEGER DEFAULT 0
     - `UNIQUE(date_local, engine_id)`
   - Add `lifetime_metrics` table:
     - Enforced singleton: `id INTEGER PRIMARY KEY CHECK (id = 1)`
     - Columns: `total_words`, `total_dictations`, `first_use_date`,
       `current_streak_days`, `longest_streak_days`, `typing_baseline_wpm`
   - **Time semantics:**
     - `date_local` is the calendar date in the user's local timezone at
       dictation time. This is the day-boundary definition for all metrics.
     - Local timezone is read from the OS at dictation time (not stored).
     - If the user travels across timezones: dictations are bucketed by the
       local date where/when they happen. This may cause a streak to "skip"
       a date if the user crosses the date line, which is acceptable.
     - Week boundaries: Monday = start of week (ISO 8601).
   - CRUD in `src-tauri/src/db/metrics.rs`

2. **Real-time WPM calculation**
   - During dictation: track elapsed time and word count
   - WPM = (word_count / elapsed_seconds) * 60
   - Emit WPM as a Tauri event during dictation (update every second)
   - Store final WPM in the dictation history entry

3. **Metrics recording**
   - After each dictation:
     - Increment daily dictation count and word total
     - Update per-engine daily breakdown in `metrics_daily_engine`
     - Update `total_wpm_weighted_sum` += (this_session_word_count * this_session_wpm)
     - **Daily avg WPM** = `total_wpm_weighted_sum / total_words` (weighted by
       word count, so longer dictations count proportionally more)
     - **Lifetime avg WPM** = same formula across all days
     - Update lifetime totals
     - Check and update streak (consecutive days with at least one dictation)
   - Skip metrics recording if private mode was active

4. **Streak tracking**
   - A streak is consecutive **local calendar days** with at least one dictation
   - Day boundary: determined by the user's local timezone at dictation time
   - On each dictation: compute today's local date. If `date_local` ==
     yesterday's `date_local` from the most recent dictation + 1 day: streak
     continues. If same day: no change. If gap > 1 day: streak resets to 1.
   - Store current streak and longest-ever streak
   - **Edge case:** if the user travels and the local date jumps (e.g., crossing
     the international date line), treat the gap as a streak break. This is
     acceptable -- streaks are motivational, not contractual.

5. **Verify:**
   - Dictation increments daily and lifetime counters
   - WPM is calculated and stored per dictation
   - Streak tracking works across days
   - Private mode dictations do not affect metrics

### Files Created

```
src-tauri/src/db/metrics.rs
src-tauri/src/metrics/
  mod.rs          # metrics collection, WPM calculation, streak logic
```

---

## Phase 2: Metrics Dashboard

Visual display of usage stats, streaks, and fun milestones.

### Work Items

1. **Dashboard page**
   - Create `src/lib/components/Dashboard.svelte`
   - Sections:
     - **Today**: dictation count, words dictated, average WPM
     - **This Week**: daily bar chart or summary, total words
     - **Lifetime**: total words, total dictations, average WPM, first use date
     - **Streak**: current streak (days), longest streak
     - **Engine Usage**: breakdown by engine (pie chart or bar)
   - All metrics data available as text (not only charts) for screen readers

2. **Real-time WPM display**
   - During active dictation: show live WPM counter
   - Location: overlay indicator or tray tooltip
   - Updates every second based on streaming word count

3. **Speaking vs. typing comparison**
   - Settings: "Typing baseline WPM" (user enters their typing speed)
   - Dashboard shows: "You dictate X% faster/slower than you type"
   - If no baseline set: show a prompt to set one

4. **Milestones and achievements**
   - Define milestones:
     - Word milestones: 1K, 5K, 10K, 25K, 50K, 100K words
     - Dictation milestones: 100, 500, 1K, 5K dictations
     - Streak milestones: 7, 14, 30, 60, 90, 365 days
   - Show earned milestones as badges in the dashboard
   - Milestones are discoverable in the dashboard only -- no pop-ups or notifications
   - Unearned milestones shown as locked/grayed with progress bar

5. **Navigation**
   - Add Dashboard to the main navigation: Dictation | Models | History | Dashboard | Settings
   - Dashboard is the "fun" page -- stats, badges, bragging rights

6. **Verify:**
   - Dashboard shows accurate counts after several dictations
   - Weekly breakdown shows the correct days
   - Streaks track correctly
   - Milestones unlock at the right thresholds
   - All chart data is accessible as text
   - Live WPM shows during dictation

### Files Created

```
src/lib/components/
  Dashboard.svelte
  MetricsCard.svelte
  MilestonesBadges.svelte
  WpmDisplay.svelte
```

---

## Phase 3: Theme Support

Dark, light, and high-contrast themes with OS preference detection.

### Work Items

1. **Theme system**
   - Define three themes: dark, light, high-contrast
   - **Default: Dark** (per product spec: "Dark mode by default"). The
     setting defaults to Dark, not Auto. Users can switch to Auto if they
     want OS-following behavior.
   - Implementation: CSS custom properties set at the root level
   - **Tailwind v4 approach (CSS-first, no v3-style config):**
     - Use `@theme` block in `app.css` to define design token values
     - Theme switching: set a class on `<html>` element (`dark`, `light`,
       `high-contrast`). Use Tailwind v4's `@variant` directive to define
       the `dark:` variant as `.dark &` (class-based, not media-query-based):
       ```css
       @custom-variant dark (&:where(.dark, .dark *));
       ```
     - Do NOT use Tailwind v3's `darkMode: 'class'` in a config file --
       Tailwind v4 uses the CSS-first approach.

2. **OS preference detection**
   - Detect `prefers-color-scheme: dark/light` via CSS media query
   - Detect `prefers-contrast: more` for high-contrast mode
   - Setting: "Theme" (Dark / Light / High Contrast / Auto)
   - Default: Dark. Auto = follow OS preference, manual = override.
   - When set to Auto: if OS reports `prefers-contrast: more`, select
     high-contrast. Otherwise follow `prefers-color-scheme`.

3. **OS text scaling**
   - Respect the system's text scaling / zoom level
   - Use `rem` units (not `px`) for all font sizes
   - Test at 100%, 125%, 150%, 200% scale factors
   - Ensure layouts don't break at larger scales

4. **Theme design tokens**
   - Define in `app.css`:
     - Background colors (primary, secondary, surface)
     - Text colors (primary, secondary, muted)
     - Accent colors (interactive elements, focus rings)
     - Border colors
     - Status colors (recording: red, processing: amber, success: green)
   - High-contrast theme: strong borders, no subtle grays, WCAG AAA contrast ratios

5. **Verify:**
   - All three themes render correctly across all pages
   - Switching themes in settings takes effect immediately
   - Auto mode follows OS preference changes
   - High-contrast mode meets WCAG AAA contrast ratios
   - Layouts work at 200% text scaling

### Files Created/Modified

```
src/app.css                     # modified: theme tokens, dark/light/high-contrast
src/lib/theme/
  index.ts                      # theme switching logic, OS detection
```

---

## Phase 4: Accessibility Audit

Comprehensive verification that all UI surfaces are accessible.

### Work Items

1. **Keyboard navigation audit**
   - Every page: verify complete keyboard navigability
   - Tab order is logical and follows visual layout
   - All interactive elements reachable via Tab
   - Escape closes modals and dropdowns
   - Arrow keys navigate within lists, menus, and complex widgets
   - Focus is visible (focus ring or outline on all interactive elements)

2. **Screen reader audit**
   - Test with VoiceOver (macOS), NVDA (Windows), Orca (Linux)
   - All interactive elements have accessible names
   - ARIA roles on custom components (buttons, dialogs, tabs, alerts)
   - Status changes announced (dictation state, download progress)
   - Charts and metrics have text alternatives
   - Live regions for real-time updates (WPM, recording timer)

3. **Focus management**
   - Modal dialogs trap focus (Tab doesn't leave the modal)
   - Opening a page moves focus to the page heading
   - Closing a dialog returns focus to the trigger element
   - Overlay indicator doesn't participate in tab order

4. **Fix identified issues**
   - Any component from M1-M10 that fails the audit gets fixed here
   - Add missing ARIA attributes, fix tab order, add keyboard handlers
   - Document the accessibility pattern for future development

5. **Settings export/import polish**
   - Ensure export/import dialogs are accessible
   - File picker uses native OS dialog (inherently accessible)
   - Success/error feedback announced to screen readers

6. **Verify:**
   - All pages pass keyboard-only navigation test
   - VoiceOver (macOS) reads all content and announces state changes
   - NVDA (Windows): same verification
   - Orca (Linux): same verification
   - Focus trapping works in all modal dialogs
   - WCAG 2.1 AA compliance across all UI surfaces

---

## Acceptance Criteria

M11 is complete when all of the following are true:

- [ ] Metrics record dictation count, word count, WPM, and engine usage per day
- [ ] Lifetime metrics track totals and streaks
- [ ] Dashboard shows daily, weekly, and lifetime stats
- [ ] Real-time WPM display during dictation
- [ ] Speaking vs. typing comparison works with user-set baseline
- [ ] Milestones unlock at correct thresholds and display in dashboard
- [ ] All metrics are available as text (not only charts)
- [ ] Dark, light, and high-contrast themes work correctly
- [ ] Theme follows OS preference in auto mode
- [ ] UI scales correctly at 200% text size
- [ ] High-contrast theme meets WCAG AAA contrast ratios
- [ ] Keyboard navigation works on every page
- [ ] Screen reader (VoiceOver, NVDA, Orca) reads all content correctly
- [ ] Focus management: trap in modals, return on close, logical tab order
- [ ] Settings export/import is accessible
- [ ] Private mode dictations do NOT record metrics (verified: counters unchanged)
- [ ] `metrics_daily.date_local` is UNIQUE (no duplicate day rows)
- [ ] `lifetime_metrics` enforces singleton (id = 1)
- [ ] Streak logic uses local timezone day boundaries
- [ ] WPM calculation uses weighted average (longer dictations count more)
- [ ] Theme default is Dark (not Auto) per product spec
- [ ] Tailwind v4 CSS-first theme switching (no v3-style config)
- [ ] All new UI strings externalized in i18n resource files
- [ ] `./scripts/agent/check` passes
- [ ] CI builds pass on all platforms
