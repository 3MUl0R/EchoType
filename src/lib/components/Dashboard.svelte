<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { untrack } from "svelte";
  import { t } from "$lib/i18n/index.js";

  interface DailyMetrics {
    date_local: string;
    dictation_count: number;
    total_words: number;
    total_duration_ms: number;
    total_wpm_weighted_sum: number;
  }

  interface HourlyActivity {
    hour: number;
    dictation_count: number;
    total_words: number;
  }

  interface DailyEngineMetrics {
    date_local: string;
    engine_id: string;
    dictation_count: number;
    total_words: number;
    total_duration_ms: number;
  }

  interface EngineLatencyStats {
    engine_id: string;
    sample_count: number;
    avg_processing_ms: number;
    avg_network_ms: number;
    avg_transcription_ms: number;
    avg_insertion_ms: number;
    avg_total_ms: number;
    p50_total_ms: number;
    p95_total_ms: number;
    avg_audio_duration_ms: number;
    avg_word_count: number;
    processing_per_sec: number;
    transcription_per_sec: number;
  }

  interface AggregatedEngine {
    engine_id: string;
    total_words: number;
    dictation_count: number;
  }

  interface LifetimeMetrics {
    total_words: number;
    total_dictations: number;
    total_wpm_weighted_sum: number;
    first_use_date: string | null;
    current_streak_days: number;
    longest_streak_days: number;
    typing_baseline_wpm: number | null;
  }

  interface Milestone {
    id: string;
    label: string;
    target: number;
    current: number;
    earned: boolean;
  }

  let today: DailyMetrics | null = $state(null);
  let weekDays: DailyMetrics[] = $state([]);
  let hourlyData: HourlyActivity[] = $state([]);
  let lifetime: LifetimeMetrics = $state({
    total_words: 0,
    total_dictations: 0,
    total_wpm_weighted_sum: 0,
    first_use_date: null,
    current_streak_days: 0,
    longest_streak_days: 0,
    typing_baseline_wpm: null,
  });
  let engineBreakdown: AggregatedEngine[] = $state([]);
  let latencyStats: EngineLatencyStats[] = $state([]);
  let baselineInput = $state("");
  let editingBaseline = $state(false);

  function avgWpm(m: DailyMetrics | null): number {
    if (!m || m.total_words === 0) return 0;
    return Math.round(m.total_wpm_weighted_sum / m.total_words);
  }

  function lifetimeAvgWpm(): number {
    if (lifetime.total_words === 0) return 0;
    return Math.round(lifetime.total_wpm_weighted_sum / lifetime.total_words);
  }

  function getMilestones(): Milestone[] {
    const words = lifetime.total_words;
    const dictations = lifetime.total_dictations;
    const streak = lifetime.longest_streak_days;

    return [
      {
        id: "words-1k",
        label: t("dashboard.milestone_words", { n: "1K" }),
        target: 1000,
        current: words,
        earned: words >= 1000,
      },
      {
        id: "words-5k",
        label: t("dashboard.milestone_words", { n: "5K" }),
        target: 5000,
        current: words,
        earned: words >= 5000,
      },
      {
        id: "words-10k",
        label: t("dashboard.milestone_words", { n: "10K" }),
        target: 10000,
        current: words,
        earned: words >= 10000,
      },
      {
        id: "words-25k",
        label: t("dashboard.milestone_words", { n: "25K" }),
        target: 25000,
        current: words,
        earned: words >= 25000,
      },
      {
        id: "words-50k",
        label: t("dashboard.milestone_words", { n: "50K" }),
        target: 50000,
        current: words,
        earned: words >= 50000,
      },
      {
        id: "words-100k",
        label: t("dashboard.milestone_words", { n: "100K" }),
        target: 100000,
        current: words,
        earned: words >= 100000,
      },
      {
        id: "dict-100",
        label: t("dashboard.milestone_dictations", { n: "100" }),
        target: 100,
        current: dictations,
        earned: dictations >= 100,
      },
      {
        id: "dict-500",
        label: t("dashboard.milestone_dictations", { n: "500" }),
        target: 500,
        current: dictations,
        earned: dictations >= 500,
      },
      {
        id: "dict-1k",
        label: t("dashboard.milestone_dictations", { n: "1K" }),
        target: 1000,
        current: dictations,
        earned: dictations >= 1000,
      },
      {
        id: "dict-5k",
        label: t("dashboard.milestone_dictations", { n: "5K" }),
        target: 5000,
        current: dictations,
        earned: dictations >= 5000,
      },
      {
        id: "streak-7",
        label: t("dashboard.milestone_streak", { n: "7" }),
        target: 7,
        current: streak,
        earned: streak >= 7,
      },
      {
        id: "streak-14",
        label: t("dashboard.milestone_streak", { n: "14" }),
        target: 14,
        current: streak,
        earned: streak >= 14,
      },
      {
        id: "streak-30",
        label: t("dashboard.milestone_streak", { n: "30" }),
        target: 30,
        current: streak,
        earned: streak >= 30,
      },
      {
        id: "streak-60",
        label: t("dashboard.milestone_streak", { n: "60" }),
        target: 60,
        current: streak,
        earned: streak >= 60,
      },
      {
        id: "streak-90",
        label: t("dashboard.milestone_streak", { n: "90" }),
        target: 90,
        current: streak,
        earned: streak >= 90,
      },
      {
        id: "streak-365",
        label: t("dashboard.milestone_streak", { n: "365" }),
        target: 365,
        current: streak,
        earned: streak >= 365,
      },
    ];
  }

  function getWeekDateRange(): { from: string; to: string } {
    const nowMs = Date.now();
    const todayDate = new globalThis.Date(nowMs);
    // Monday = start of week (ISO 8601)
    const day = todayDate.getDay();
    const diff = day === 0 ? 6 : day - 1;
    const mondayMs = nowMs - diff * 86_400_000;
    const sundayMs = mondayMs + 6 * 86_400_000;
    // Format using local date parts (not toISOString which converts to UTC)
    const fmt = (ms: number) => {
      const d = new globalThis.Date(ms);
      const y = d.getFullYear();
      const m = String(d.getMonth() + 1).padStart(2, "0");
      const dd = String(d.getDate()).padStart(2, "0");
      return `${y}-${m}-${dd}`;
    };
    return { from: fmt(mondayMs), to: fmt(sundayMs) };
  }

  function formatDate(iso: string): string {
    const d = new Date(iso + "T00:00:00");
    return d.toLocaleDateString(undefined, { weekday: "short" });
  }

  function formatHour(h: number): string {
    if (h === 0) return "12a";
    if (h < 12) return `${h}a`;
    if (h === 12) return "12p";
    return `${h - 12}p`;
  }

  function progressPercent(current: number, target: number): number {
    return Math.min(100, Math.round((current / target) * 100));
  }

  async function loadMetrics() {
    try {
      today = await invoke<DailyMetrics | null>("get_metrics_today");
      lifetime = await invoke<LifetimeMetrics>("get_lifetime_metrics");

      const { from, to } = getWeekDateRange();
      weekDays = await invoke<DailyMetrics[]>("get_metrics_range", {
        from,
        to,
      });
      // Hourly usage patterns (all-time)
      const rawHourly = await invoke<HourlyActivity[]>(
        "get_hourly_activity",
        { from: "2000-01-01", to: "2099-12-31" },
      );
      // Fill all 24 hours, merging with data
      const hourMap = new Map(rawHourly.map((h) => [h.hour, h]));
      hourlyData = Array.from({ length: 24 }, (_, i) => hourMap.get(i) ?? { hour: i, dictation_count: 0, total_words: 0 });

      const rawEngines = await invoke<DailyEngineMetrics[]>(
        "get_engine_breakdown",
        { from: "2000-01-01", to: "2099-12-31" },
      );
      // Aggregate per-day rows into per-engine totals
      const engineAcc: Record<string, AggregatedEngine> = {};
      for (const row of rawEngines) {
        const existing = engineAcc[row.engine_id];
        if (existing) {
          existing.total_words += row.total_words;
          existing.dictation_count += row.dictation_count;
        } else {
          engineAcc[row.engine_id] = {
            engine_id: row.engine_id,
            total_words: row.total_words,
            dictation_count: row.dictation_count,
          };
        }
      }
      engineBreakdown = Object.values(engineAcc);

      // Latency profiling stats per engine
      latencyStats = await invoke<EngineLatencyStats[]>("get_latency_stats");
    } catch (e) {
      console.error("Failed to load metrics:", e);
    }
  }

  async function saveBaseline() {
    const wpm = parseFloat(baselineInput);
    if (isNaN(wpm) || wpm <= 0 || wpm > 500) return;
    try {
      await invoke("set_typing_baseline", { wpm });
      lifetime.typing_baseline_wpm = wpm;
      editingBaseline = false;
    } catch (e) {
      console.error("Failed to save baseline:", e);
    }
  }

  // Load on mount (untrack to avoid re-triggering on state updates)
  $effect(() => {
    untrack(() => loadMetrics());
  });

  // Refresh metrics after each dictation completes
  $effect(() => {
    const unlisten = listen("dictation:state", (event) => {
      const data = event.payload as { state: string };
      if (data.state === "idle") {
        loadMetrics();
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<div class="mx-auto max-w-3xl px-6 py-6">
  <h2 class="mb-6 text-lg font-semibold">{t("dashboard.title")}</h2>

  <!-- Today -->
  <section class="mb-6 rounded-lg bg-bg-secondary p-4" aria-label={t("dashboard.today")}>
    <h3 class="mb-3 text-sm font-medium text-text-secondary">{t("dashboard.today")}</h3>
    <div class="grid grid-cols-3 gap-4 text-center">
      <div>
        <p class="text-2xl font-bold">{today?.dictation_count ?? 0}</p>
        <p class="text-xs text-text-muted">{t("dashboard.dictations")}</p>
      </div>
      <div>
        <p class="text-2xl font-bold">{today?.total_words ?? 0}</p>
        <p class="text-xs text-text-muted">{t("dashboard.words")}</p>
      </div>
      <div>
        <p class="text-2xl font-bold">{avgWpm(today)}</p>
        <p class="text-xs text-text-muted">{t("dashboard.avg_wpm")}</p>
      </div>
    </div>
  </section>

  <!-- This Week -->
  <section class="mb-6 rounded-lg bg-bg-secondary p-4" aria-label={t("dashboard.this_week")}>
    <h3 class="mb-3 text-sm font-medium text-text-secondary">{t("dashboard.this_week")}</h3>
    {#if weekDays.length > 0}
      <div class="flex items-end gap-2" role="img" aria-label={t("dashboard.weekly_chart_alt", {
        days: weekDays.length.toString(),
        total: weekDays.reduce((s, d) => s + d.total_words, 0).toString()
      })}>
        {#each weekDays as day (day.date_local)}
          {@const maxWords = Math.max(...weekDays.map((d) => d.total_words), 1)}
          {@const height = Math.max(4, (day.total_words / maxWords) * 80)}
          <div class="flex flex-1 flex-col items-center gap-1">
            <span class="text-xs text-text-muted">{day.total_words}</span>
            <div
              class="w-full rounded bg-accent"
              style="height: {height}px"
            ></div>
            <span class="text-xs text-text-muted">{formatDate(day.date_local)}</span>
          </div>
        {/each}
      </div>
      <p class="mt-2 text-xs text-text-muted">
        {t("dashboard.week_total", { words: weekDays.reduce((s, d) => s + d.total_words, 0).toString() })}
      </p>
    {:else}
      <p class="text-sm text-text-muted">{t("dashboard.no_data_week")}</p>
    {/if}
  </section>

  <!-- Hourly Usage Patterns -->
  <section class="mb-6 rounded-lg bg-bg-secondary p-4" aria-label={t("dashboard.usage_patterns")}>
    <h3 class="mb-3 text-sm font-medium text-text-secondary">{t("dashboard.usage_patterns")}</h3>
    {#if hourlyData.reduce((s, h) => s + h.dictation_count, 0) > 0}
      {@const totalHourlyCount = hourlyData.reduce((s, h) => s + h.dictation_count, 0)}
      {@const maxCount = Math.max(...hourlyData.map((h) => h.dictation_count), 1)}
      <div class="flex items-end gap-px" role="img" aria-label={t("dashboard.hourly_chart_alt", {
        total: totalHourlyCount.toString()
      })}>
        {#each hourlyData as bucket (bucket.hour)}
          {@const height = Math.max(2, (bucket.dictation_count / maxCount) * 64)}
          {@const opacity = bucket.dictation_count > 0 ? 0.4 + (bucket.dictation_count / maxCount) * 0.6 : 0.15}
          <div class="flex flex-1 flex-col items-center gap-0.5" title="{bucket.dictation_count} dictations, {bucket.total_words} words">
            <div
              class="w-full rounded-sm bg-accent"
              style="height: {height}px; opacity: {opacity}"
            ></div>
            {#if bucket.hour % 3 === 0}
              <span class="text-[9px] text-text-muted">{formatHour(bucket.hour)}</span>
            {/if}
          </div>
        {/each}
      </div>
      <p class="mt-2 text-xs text-text-muted">
        {t("dashboard.hourly_total", {
          count: totalHourlyCount.toString(),
          hours: hourlyData.filter((h) => h.dictation_count > 0).length.toString()
        })}
      </p>
    {:else}
      <p class="text-sm text-text-muted">{t("dashboard.no_hourly_data")}</p>
    {/if}
  </section>

  <!-- Lifetime -->
  <section class="mb-6 rounded-lg bg-bg-secondary p-4" aria-label={t("dashboard.lifetime")}>
    <h3 class="mb-3 text-sm font-medium text-text-secondary">{t("dashboard.lifetime")}</h3>
    <div class="grid grid-cols-2 gap-4">
      <div>
        <p class="text-xl font-bold">{lifetime.total_words.toLocaleString()}</p>
        <p class="text-xs text-text-muted">{t("dashboard.total_words")}</p>
      </div>
      <div>
        <p class="text-xl font-bold">{lifetime.total_dictations.toLocaleString()}</p>
        <p class="text-xs text-text-muted">{t("dashboard.total_dictations")}</p>
      </div>
      <div>
        <p class="text-xl font-bold">{lifetimeAvgWpm()}</p>
        <p class="text-xs text-text-muted">{t("dashboard.avg_wpm")}</p>
      </div>
      <div>
        <p class="text-xl font-bold">{lifetime.first_use_date ?? "—"}</p>
        <p class="text-xs text-text-muted">{t("dashboard.first_use")}</p>
      </div>
    </div>
  </section>

  <!-- Streak -->
  <section class="mb-6 rounded-lg bg-bg-secondary p-4" aria-label={t("dashboard.streak")}>
    <h3 class="mb-3 text-sm font-medium text-text-secondary">{t("dashboard.streak")}</h3>
    <div class="grid grid-cols-2 gap-4 text-center">
      <div>
        <p class="text-3xl font-bold">{lifetime.current_streak_days}</p>
        <p class="text-xs text-text-muted">{t("dashboard.current_streak")}</p>
      </div>
      <div>
        <p class="text-3xl font-bold">{lifetime.longest_streak_days}</p>
        <p class="text-xs text-text-muted">{t("dashboard.longest_streak")}</p>
      </div>
    </div>
  </section>

  <!-- Speaking vs Typing -->
  <section class="mb-6 rounded-lg bg-bg-secondary p-4" aria-label={t("dashboard.comparison")}>
    <h3 class="mb-3 text-sm font-medium text-text-secondary">{t("dashboard.comparison")}</h3>
    {#if lifetime.typing_baseline_wpm}
      {@const dictWpm = lifetimeAvgWpm()}
      {@const baseline = lifetime.typing_baseline_wpm}
      {@const diff = dictWpm > 0 ? Math.round(((dictWpm - baseline) / baseline) * 100) : 0}
      <p class="text-sm">
        {t("dashboard.baseline_value", { wpm: baseline.toString() })}
      </p>
      {#if dictWpm > 0}
        <p class="mt-1 text-sm {diff >= 0 ? 'text-status-success' : 'text-status-recording'}">
          {diff >= 0
            ? t("dashboard.faster_than_typing", { percent: diff.toString() })
            : t("dashboard.slower_than_typing", { percent: Math.abs(diff).toString() })}
        </p>
      {/if}
      <button
        class="mt-2 text-xs text-accent hover:text-accent-hover"
        onclick={() => {
          editingBaseline = true;
          baselineInput = baseline.toString();
        }}
      >{t("dashboard.edit_baseline")}</button>
    {:else if editingBaseline}
      <div class="flex items-center gap-2">
        <input
          type="number"
          class="w-24 rounded border border-border bg-bg-surface px-2 py-1 text-sm"
          placeholder="WPM"
          aria-label={t("settings.typing_baseline_wpm")}
          bind:value={baselineInput}
          min="1"
          max="500"
        />
        <button
          class="rounded bg-accent px-3 py-1 text-sm text-white hover:bg-accent-hover"
          onclick={saveBaseline}
        >{t("profiles.save")}</button>
        <button
          class="text-sm text-text-muted hover:text-text-primary"
          onclick={() => (editingBaseline = false)}
        >{t("profiles.cancel")}</button>
      </div>
    {:else}
      <p class="text-sm text-text-muted">{t("dashboard.no_baseline")}</p>
      <button
        class="mt-2 text-xs text-accent hover:text-accent-hover"
        onclick={() => (editingBaseline = true)}
      >{t("dashboard.set_baseline")}</button>
    {/if}
  </section>

  <!-- Engine Usage -->
  {#if engineBreakdown.length > 0}
    <section class="mb-6 rounded-lg bg-bg-secondary p-4" aria-label={t("dashboard.engine_usage")}>
      <h3 class="mb-3 text-sm font-medium text-text-secondary">{t("dashboard.engine_usage")}</h3>
      <div class="space-y-2">
        {#each engineBreakdown as eng (eng.engine_id)}
          {@const totalEngineWords = engineBreakdown.reduce((s, e) => s + e.total_words, 0) || 1}
          {@const pct = Math.round((eng.total_words / totalEngineWords) * 100)}
          <div class="flex items-center gap-2">
            <span class="w-24 truncate text-xs">{eng.engine_id.charAt(0).toUpperCase() + eng.engine_id.slice(1)}</span>
            <div class="flex-1 rounded-full bg-bg-surface h-3">
              <div class="h-3 rounded-full bg-accent" style="width: {pct}%"></div>
            </div>
            <span class="text-xs text-text-muted w-12 text-right">{pct}%</span>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  <!-- Latency Profiling -->
  {#if latencyStats.length > 0}
    <section class="mb-6 rounded-lg bg-bg-secondary p-4" aria-label="Latency Profiling">
      <h3 class="mb-3 text-sm font-medium text-text-secondary">Latency Profiling</h3>
      <div class="space-y-4">
        {#each latencyStats as stat (stat.engine_id)}
          {@const totalAvg = stat.avg_total_ms || 1}
          {@const procPct = (stat.avg_processing_ms / totalAvg) * 100}
          {@const transPct = (stat.avg_transcription_ms / totalAvg) * 100}
          {@const insPct = (stat.avg_insertion_ms / totalAvg) * 100}
          <div class="rounded border border-border bg-bg-surface p-3">
            <div class="flex items-center justify-between mb-2">
              <span class="text-sm font-medium">{stat.engine_id.charAt(0).toUpperCase() + stat.engine_id.slice(1)}</span>
              <span class="text-xs text-text-muted">{stat.sample_count} samples</span>
            </div>
            <div class="flex h-4 w-full overflow-hidden rounded-full bg-bg-primary mb-2" title="Avg total: {Math.round(stat.avg_total_ms)}ms">
              <div class="bg-status-processing" style="width: {procPct}%" title="Processing: {Math.round(stat.avg_processing_ms)}ms"></div>
              <div class="bg-accent" style="width: {transPct}%" title="Transcription: {Math.round(stat.avg_transcription_ms)}ms"></div>
              <div class="bg-status-success" style="width: {insPct}%" title="Insertion: {Math.round(stat.avg_insertion_ms)}ms"></div>
            </div>
            <div class="grid grid-cols-3 gap-2 text-xs">
              <div class="flex items-center gap-1">
                <span class="inline-block h-2 w-2 rounded-full bg-status-processing"></span>
                <span class="text-text-muted">Process</span>
                <span class="font-medium">{Math.round(stat.processing_per_sec)}ms/s</span>
              </div>
              <div class="flex items-center gap-1">
                <span class="inline-block h-2 w-2 rounded-full bg-accent"></span>
                <span class="text-text-muted">Transcribe</span>
                <span class="font-medium">{Math.round(stat.transcription_per_sec)}ms/s</span>
              </div>
              <div class="flex items-center gap-1">
                <span class="inline-block h-2 w-2 rounded-full bg-status-success"></span>
                <span class="text-text-muted">Insert</span>
                <span class="font-medium">{Math.round(stat.avg_insertion_ms)}ms</span>
              </div>
            </div>
            <div class="mt-2 grid grid-cols-3 gap-2 text-xs text-text-muted">
              <div>
                <span class="text-text-secondary font-medium">{Math.round(stat.avg_total_ms)}ms</span> avg
              </div>
              <div>
                <span class="text-text-secondary font-medium">{stat.p50_total_ms}ms</span> p50
              </div>
              <div>
                <span class="text-text-secondary font-medium">{stat.p95_total_ms}ms</span> p95
              </div>
            </div>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  <!-- Milestones -->
  <section class="mb-6 rounded-lg bg-bg-secondary p-4" aria-label={t("dashboard.milestones")}>
    <h3 class="mb-3 text-sm font-medium text-text-secondary">{t("dashboard.milestones")}</h3>
    <div class="grid grid-cols-2 gap-3 sm:grid-cols-3">
      {#each getMilestones() as milestone (milestone.id)}
        <div
          class="rounded-lg border p-3 text-center {milestone.earned
            ? 'border-accent bg-accent/10'
            : 'border-border bg-bg-surface opacity-60'}"
          aria-label={milestone.earned
            ? t("dashboard.milestone_earned", { name: milestone.label })
            : t("dashboard.milestone_locked", { name: milestone.label, progress: progressPercent(milestone.current, milestone.target).toString() })}
        >
          <p class="text-sm font-medium {milestone.earned ? 'text-accent' : 'text-text-muted'}">
            {milestone.label}
          </p>
          {#if !milestone.earned}
            <div class="mt-1 h-1.5 rounded-full bg-bg-primary">
              <div
                class="h-1.5 rounded-full bg-accent/40"
                style="width: {progressPercent(milestone.current, milestone.target)}%"
              ></div>
            </div>
            <p class="mt-1 text-xs text-text-muted">
              {progressPercent(milestone.current, milestone.target)}%
            </p>
          {/if}
        </div>
      {/each}
    </div>
  </section>
</div>
