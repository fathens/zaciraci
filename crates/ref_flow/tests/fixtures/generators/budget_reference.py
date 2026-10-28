"""Credit budget for the ref_flow collector — constants + pure functions only.

No I/O, no env, no API key: collect.py, check_runs.py and test_budget.py all
import this module. Keep it that way (a syntax error here must not take the
watchdog down with the collector, and the watchdog must never need the key).

Policy v6 (2026-09-06, planner + financial/architecture review, see
PLAN_budget_v6.md). Requirements: never exceed the provider's monthly cap,
never starve the forward stream. Provider (NearBlocks Startup): 190/min,
25,000/day, 500,000/month. Unit = requests (reconciled 2026-10-09: a limit=100 list
page counts 1, same as a detail call; the earlier "4 credits per page" was wrong).

    day_allow = T_ALLOW (14,000) until CUTOVER (2026-09-29 00:00 UTC), then DAY_MAX (15,000)
    R(t)      = max(RESERVE_MIN, int(min(0.9*day_allow, max(NEED, fwd_need)) * (86400 - sec_of_day) / 86400))
                (fwd_need = 1.25 x 7-day mean forward credits, supplied by the caller — the
                 reserve follows measured demand instead of waiting for a human to re-derive NEED)
    forward : min(run_left, day_allow - used_today,        MONTH - calendar_month, MONTH - roll31)
    gap     : min(run_left, day_allow - R(t) - used_today, MONTH - calendar_month, MONTH - ENR_WALL_MARGIN - roll31, paced)
    enrich  : min(run_left, day_allow - R(t) - used_today, MONTH - calendar_month, MONTH - ENR_WALL_MARGIN - roll31, paced, enrich_paced)
    paced   = day_allow * sec_of_day / 86400 + PACE_HEAD - used_today

Stream classes (2026-09-12): "forward" = the live list; "gap" = catch-up of
holes (a forward run that hit its page cap, or a multi-hour outage) — it ranks
above enrich but pays the forward reserve and the day pacing, because on
2026-09-12 a 38-hour outage catch-up ran as forward-class and consumed the whole
daily allowance by 05:30 UTC, stalling forward for the rest of the day; "enrich"
= detail fetches, additionally paced on its own allotment.

Pacing applies to the gap and enrich classes: it stops enrich from burning the day's
allowance in the first hours. Forward is bounded by its own demand (~5k/day) and
by day_allow, so pacing it would only add stalls (e.g. after a sleep catch-up, or
on the v6 deploy day when the morning had already been spent at the old rate).

Enrich allotment (2026-09-10): on a heavy-forward day the reserve line R(t) and
forward's own consumption move in parallel, so `day_allow - R(t) - used_today`
stayed <= 0 from 09:30 to 19:30 and every enrich credit was spent before 09:30
(time-biased sample again). Enrich therefore gets its own paced daily share:
    enrich_allot = day_allow - fwd_need - ENRICH_SLACK      (fwd_need from the caller:
                                                             1.25 x 7-day mean forward credits)
    enrich_paced = enrich_allot * sec_of_day/86400 + enrich_allot/24 - enrich_used_today
and may never exceed it. R(t) stays as the hard guard for forward; the allotment
is what makes the enrich sample uniform over the day.

Which term guards which window (do NOT "simplify" one away — two reviewers
each nearly deleted one during the review):
    day_allow              the workhorse: 31 * DAY_MAX = 465,000 < MONTH, so every
                           month-like window is safe in steady state by construction
    MONTH - calendar_month the provider's own unit if it bills by calendar month.
                           Theoretically never binding once the roll31 wall exists
                           (calendar month <= some 31-day window) — kept as an
                           independent code path (string prefix vs date arithmetic)
    MONTH - roll31         any calendar month, contract cycle or rolling-30-day
                           window is contained in a 31-day window; consumption is
                           non-negative, so roll31 <= MONTH bounds all three
                           interpretations. Only fires on misconfiguration (e.g. a
                           hand-set 24k/day, which is exactly what happened 08-30)
    ENR_WALL_MARGIN        enrich stops FWD_TAIL earlier than forward, so a runaway
                           starves enrich first and forward survives ~1.2 more days
                           ("forward is the last thing to stop")

Neither the walls nor budget_run are hard limits: the caller checks the budget
before a fetch and a fetch may retry up to 4 times, so each can overshoot by one
fetch (<= 16 credits). The containment bound is therefore MONTH + 16.

Reserve R(t): the proportional part separates the two paced lines during the day
(the gap between them is NEED * f * (1 - f), a parabola that is zero at both ends
of the day); the floor protects the last hours when the parabola collapses. The
two parts have different jobs — keep both. NEED = 9,500 was derived from the
busiest measured day (2026-09-04: 4,920 credits of forward, right-censored by
FWD_MAX_PAGES; true need of the capped run measured at 516). Re-derived 2026-09-17
to 10,500 after receipts/day grew 107k -> 218k and forward+gap hit 10,408 on
2026-09-16 (T_ALLOW raised 12,000 -> 14,000 the same day; day_allow exhaustion,
not the reserve, was what stopped forward at 22:30 UTC). It WILL go stale:
re-derive it when check_runs warns that forward's 7-day mean exceeds 60% of
day_allow. T_ALLOW > NEED must hold so `day_allow - R(t)` cannot go negative.

Ledger: {"YYYY-MM-DD": credits} keyed by UTC day. Do not prune it below 40 days
or the rolling window silently under-counts. Unparseable / future keys are
ignored. All date arithmetic is UTC via calendar.timegm — never time.mktime
(local-time interpretation shifted ENRICH_MIN_TS by 9h in v5).

Transition: legacy days at 24,000 (08-30..09-06) sit in the window until 09-29;
T_ALLOW = 12,000 kept every 31-day window <= 465,000 through the hand-off
(400-day replay from the real ledger, worst31 = 465,000); by 2026-09-17 the
outage days (09-10/11, 09-12, 09-15) had left enough room that 14,000 gives the
same worst31 (projection from the real ledger), so T_ALLOW was raised. DAY_MAX may be revisited
on 2026-10-07 (RECONCILE_DEADLINE) once the dashboard has been reconciled with
the ledger at least once — check_runs.py flags PROBLEM if that never happens.
"""
import calendar
import datetime as _dt
import time

# --- plan table (the ONLY place plan limits live) --------------------------------
PLANS = {
    #            per-day cap   monthly cap  run cap   pace    head   need   floor  enrich wall margin
    # standard (2026-09-19): provider 500/min, 187,500/day, 3,750,000/month. day_max is set by the
    # month (3.6M/31 = 116k) not the day; run cap by wall clock (~0.75 s/request -> ~2,000/25 min).
    # v8 (2026-09-26): enrich runs ENRICH_WORKERS threads under a global RATE_PER_MIN limiter, so the
    # wall clock no longer binds; day_max 113k = as close to 3.6M/31 as the enrich wall allows.
    "standard": dict(day_max=113000, month=3600000, run=6000, pace_sec=0.0, pace_head=4000,
                     need=20000, reserve_min=2500, enr_wall_margin=40000, workers=4, rate_per_min=420, rate_per_sec=5),
    "startup": dict(day_max=15000, month=480000, run=1200, pace_sec=0.35, pace_head=2000,
                    need=10500, reserve_min=1200, enr_wall_margin=6000, workers=1, rate_per_min=160, rate_per_sec=2.5),
    "free":    dict(day_max=300,   month=9500,   run=120,  pace_sec=6.5,  pace_head=40,
                    need=120,  reserve_min=16,   enr_wall_margin=150, workers=1, rate_per_min=5, rate_per_sec=0.1),
    "keyless": dict(day_max=280,   month=9500,   run=120,  pace_sec=6.5,  pace_head=40,
                    need=110,  reserve_min=16,   enr_wall_margin=120, workers=1, rate_per_min=5, rate_per_sec=0.1),
}
PROVIDER_DAY_HARD = {"standard": 187500, "startup": 25000, "free": 333, "keyless": 333}
RUN_DEADLINE_S = 25 * 60             # a run must leave the next cron slot free (flock skips otherwise)
GAP_TIME_S = 10 * 60                 # recent-gap catch-up may use at most this much of a run (from run start);
                                     # 2026-09-27 20:00 UTC a 139k-receipt/hour bot storm made list catch-up eat
                                     # whole runs for 4.5 h and enrich got zero time. Gaps simply carry over.

# Transition allowance (startup only): lower daily cap while the legacy 24k days
# age out of the 31-day window. Self-retires at CUTOVER_TS.
T_ALLOW = {"startup": 15000}   # 12,000 -> 14,000 (09-17) -> 15,000 (09-18): projection from the real ledger gives worst31 469,112 (09-28) < 474,000; the transition is effectively retired
CUTOVER_TS = calendar.timegm((2026, 9, 29, 0, 0, 0))
RECONCILE_DEADLINE = "2026-10-07"   # decide DAY_MAX (keep 15,000 / drop to 14,000) after 1 reconcile
ROLL_DAYS = 31
LEDGER_MIN_KEEP_DAYS = 40
FETCH_OVERSHOOT = 16                 # 4 credits x 4 attempts
ENRICH_SLACK = 600                   # credits/day left unallocated between forward need and enrich share
FWD_NEED_FLOOR = 4000                # never assume forward needs less than this per day
FWD_NEED_FACTOR = 1.25               # margin over the 7-day mean forward spend
FWD_NEED_LAST_FACTOR = 1.15          # margin over the last full day (reacts to a growth spurt within a day)

def plan_params(plan):
    return PLANS.get(plan, PLANS["keyless"])

# --- invariants (fail loudly at import if someone edits a constant carelessly) ----
for _name, _p in PLANS.items():
    _t = T_ALLOW.get(_name, _p["day_max"])
    assert _t <= _p["day_max"] <= PROVIDER_DAY_HARD[_name], (_name, "day cap vs provider hard limit")
    assert ROLL_DAYS * _p["day_max"] < _p["month"] - _p["enr_wall_margin"], (_name, "31*DAY_MAX must clear the enrich wall")
    assert _t > _p["need"] > _p["reserve_min"] > 0, (_name, "T_ALLOW > NEED > RESERVE_MIN")
    assert 0 < _p["enr_wall_margin"] < _p["month"] * 0.1, (_name, "enrich wall margin")

# --- time helpers (UTC only) --------------------------------------------------------
def day_key(now_ts):
    return time.strftime("%Y-%m-%d", time.gmtime(now_ts))

def month_key(now_ts):
    return time.strftime("%Y-%m", time.gmtime(now_ts))

def sec_of_day(now_ts):
    t = time.gmtime(now_ts)
    return t.tm_hour * 3600 + t.tm_min * 60 + t.tm_sec

def _parse_day(key):
    try:
        return _dt.date.fromisoformat(key)
    except (TypeError, ValueError):
        return None

# --- ledger aggregates ---------------------------------------------------------------
def roll_sum(ledger, now_ts, days=ROLL_DAYS):
    """Sum of the last `days` UTC days INCLUDING today. Ignores unparseable and
    future keys. Today is included because this feeds the walls (a bound on
    total consumption), not the daily allocator."""
    today = _parse_day(day_key(now_ts))
    start = today - _dt.timedelta(days=days - 1)
    total = 0
    for k, v in ledger.items():
        d = _parse_day(k)
        if d is not None and start <= d <= today:
            total += int(v)
    return total

def used_calendar_month(ledger, now_ts):
    mk = month_key(now_ts)
    return sum(int(v) for k, v in ledger.items() if isinstance(k, str) and k.startswith(mk))

def used_today(ledger, now_ts):
    return int(ledger.get(day_key(now_ts), 0))

# --- policy ------------------------------------------------------------------------------
def day_allowance(plan, now_ts):
    p = plan_params(plan)
    t = T_ALLOW.get(plan)
    if t is not None and now_ts < CUTOVER_TS:
        return t, "transition"
    return p["day_max"], "steady"

def forward_need(fwd_7d_mean, fwd_last_day=0):
    """Daily credits set aside for forward when sizing the enrich share and the
    reserve: the larger of 1.25 x 7-day mean and 1.15 x the last full day, so a
    growth spurt (2026-09-13..17: receipts 91k -> 251k/day) thins enrich the
    next day instead of a week later. Enrich is opportunistic; forward is not."""
    return max(FWD_NEED_FLOOR, int((fwd_7d_mean or 0) * FWD_NEED_FACTOR),
               int((fwd_last_day or 0) * FWD_NEED_LAST_FACTOR))

def enrich_allotment(day_allow, fwd_need):
    return max(int(day_allow - fwd_need - ENRICH_SLACK), 0)

def reserve(plan, now_ts, day_allow, fwd_need=None):
    """Credits kept back from enrich-class passes for the rest of the UTC day.
    The daily need is the larger of the plan constant and the measured forward
    demand (`fwd_need`, see forward_need()), capped at 0.9 x day_allow."""
    p = plan_params(plan)
    remaining = 86400 - sec_of_day(now_ts)
    need = min(0.9 * day_allow, max(p["need"], fwd_need or 0))
    prop = int(need * remaining / 86400)
    return max(p["reserve_min"], prop)

def compute_budget(ledger, *, plan, now_ts, calls_this_run, stream, budget_run,
                   no_pace=False, rate_limited=False, net_down=False,
                   fwd_need=None, enrich_used_today=None):
    """Credits the caller may still spend right now. `stream` is "forward"
    (forward list + recent-gap continuations) or "enrich" (enrich, old gaps,
    backfill). When `fwd_need` and `enrich_used_today` are given, enrich-class
    spending is additionally paced against its own daily allotment. Never
    negative."""
    if rate_limited or net_down:
        return 0
    p = plan_params(plan)
    day_allow, _mode = day_allowance(plan, now_ts)
    used_day = used_today(ledger, now_ts)
    r31 = roll_sum(ledger, now_ts, ROLL_DAYS)
    month_used = used_calendar_month(ledger, now_ts)
    if stream == "forward":
        day_cap = day_allow
        wall = p["month"]
    else:
        day_cap = day_allow - reserve(plan, now_ts, day_allow, fwd_need)
        wall = p["month"] - p["enr_wall_margin"]
    left = min(budget_run - calls_this_run,
               day_cap - used_day,
               p["month"] - month_used,
               wall - r31)
    if not no_pace and stream != "forward":
        paced = int(day_allow * sec_of_day(now_ts) / 86400) + p["pace_head"] - used_day
        left = min(left, paced)
        if stream == "enrich" and fwd_need is not None and enrich_used_today is not None:
            allot = enrich_allotment(day_allow, fwd_need)
            e_paced = int(allot * sec_of_day(now_ts) / 86400) + allot // 24 - int(enrich_used_today)
            left = min(left, e_paced)   # total is bounded by allot + allot/24 at end of day
    return max(int(left), 0)

def report(ledger, plan, now_ts, fwd_need=None, enrich_used_today=None):
    """Observability snapshot for the run-start log and check_runs."""
    p = plan_params(plan)
    day_allow, mode = day_allowance(plan, now_ts)
    return {
        "enrich_allot": enrich_allotment(day_allow, fwd_need) if fwd_need is not None else None,
        "enrich_used": enrich_used_today,
        "day_allow": day_allow,
        "mode": mode,
        "used_today": used_today(ledger, now_ts),
        "roll31": roll_sum(ledger, now_ts, ROLL_DAYS),
        "month_used": used_calendar_month(ledger, now_ts),
        "month_cap": p["month"],
        "enr_wall": p["month"] - p["enr_wall_margin"],
        "reserve_now": reserve(plan, now_ts, day_allow, fwd_need),
    }

def format_report(rep):
    return (f"budget: day_allow={rep['day_allow']} mode={rep['mode']} used_today={rep['used_today']} "
            f"roll31={rep['roll31']}/{rep['month_cap']} (enrich wall {rep['enr_wall']}) "
            f"month={rep['month_used']}/{rep['month_cap']} reserve_now={rep['reserve_now']}"
            + (f" enrich={rep['enrich_used']}/{rep['enrich_allot']}" if rep.get('enrich_allot') is not None else ""))
