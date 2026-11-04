#!/usr/bin/env python3
"""Generate budget_golden.json from the frozen Python reference policy.

Run once (seeded, deterministic) and commit the output; cargo test never
invokes Python. Re-run only when budget_reference.py changes on purpose:

    python3 -I gen_budget_golden.py > ../budget_golden.json
"""
import calendar
import datetime as dt
import json
import os
import random
import sys

# -I drops the script directory from sys.path; the reference module lives next to us.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import budget_reference as B  # noqa: E402

PLAN = "standard"
P = B.PLANS[PLAN]
SEED = 20261026
N_RANDOM = 2500

rng = random.Random(SEED)


def ts(y, m, d, h=0, mi=0, s=0):
    return calendar.timegm((y, m, d, h, mi, s))


def iso(now_ts):
    return dt.datetime.fromtimestamp(now_ts, dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S")


def random_ledger(today, max_days=45):
    """Ledger over the last `max_days` days (some days missing), values in
    the range the real ledger shows: 0 .. day_max, occasional overshoot."""
    out = {}
    for i in range(rng.randint(0, max_days)):
        day = today - dt.timedelta(days=i)
        if rng.random() < 0.15:
            continue
        roll = rng.random()
        if roll < 0.6:
            v = rng.randint(0, P["day_max"])
        elif roll < 0.9:
            v = rng.randint(90_000, 113_000)
        else:
            v = rng.randint(113_000, 190_000)  # misconfiguration / legacy overshoot
        out[day.isoformat()] = v
    return out


def one_case(now_ts, ledger, stream, calls, no_pace, rate_limited, net_down, fwd_need, enrich_used):
    budget = B.compute_budget(
        ledger, plan=PLAN, now_ts=now_ts, calls_this_run=calls, stream=stream,
        budget_run=P["run"], no_pace=no_pace, rate_limited=rate_limited, net_down=net_down,
        fwd_need=fwd_need, enrich_used_today=enrich_used,
    )
    day_allow, _ = B.day_allowance(PLAN, now_ts)
    return {
        "ledger": ledger,
        "now": iso(now_ts),
        "calls": calls,
        "stream": stream,
        "no_pace": no_pace,
        "rate_limited": rate_limited,
        "net_down": net_down,
        "fwd_need": fwd_need,
        "enrich_used_today": enrich_used,
        "expect": {
            "budget": budget,
            "reserve": B.reserve(PLAN, now_ts, day_allow, fwd_need),
            "used_today": B.used_today(ledger, now_ts),
            "roll31": B.roll_sum(ledger, now_ts, B.ROLL_DAYS),
            "month_used": B.used_calendar_month(ledger, now_ts),
            "day_allow": day_allow,
        },
    }


def random_now():
    base = ts(2026, 9, 29)
    day = rng.randint(0, 400)
    roll = rng.random()
    if roll < 0.05:
        sec = 0
    elif roll < 0.10:
        sec = 86399
    elif roll < 0.15:
        sec = 86400 - rng.randint(1, 60)
    else:
        sec = rng.randint(0, 86399)
    return base + day * 86400 + sec


cases = []
# Boundary sweep: every stream at 00:00:00, 00:00:01, 23:59:59 and each hour with
# a ledger sitting right at the interesting walls.
today = dt.date(2026, 10, 26)
near_wall = {
    (today - dt.timedelta(days=i)).isoformat(): (P["month"] - P["enr_wall_margin"]) // 30
    for i in range(1, 31)
}
near_wall[today.isoformat()] = 0
for stream in ("forward", "gap", "enrich"):
    for sec in [0, 1, 3600 * 12, 86399] + [h * 3600 for h in range(24)]:
        now_ts = ts(2026, 10, 26) + sec
        for ledger in ({}, {today.isoformat(): P["day_max"] - 10}, {today.isoformat(): P["day_max"]}, near_wall,
                       {"2026-10-01": P["month"]}):
            for no_pace in (False, True):
                cases.append(one_case(now_ts, dict(ledger), stream, 0, no_pace, False, False,
                                      B.forward_need(11000, 18000), 1234))
for _ in range(N_RANDOM):
    now_ts = random_now()
    today = dt.date.fromisoformat(B.day_key(now_ts))
    ledger = random_ledger(today)
    stream = rng.choice(["forward", "gap", "enrich"])
    calls = rng.choice([0, 0, rng.randint(0, P["run"]), P["run"]])
    no_pace = rng.random() < 0.2
    rate_limited = rng.random() < 0.03
    net_down = rng.random() < 0.03
    fwd_need = None if rng.random() < 0.2 else rng.randint(0, 40_000)
    enrich_used = None if rng.random() < 0.2 else rng.randint(0, 120_000)
    cases.append(one_case(now_ts, ledger, stream, calls, no_pace, rate_limited, net_down, fwd_need, enrich_used))

forward_need_cases = [
    {"mean": m, "last": l, "expect": B.forward_need(m, l)}
    for m, l in [(0, 0), (5500, 0), (9000, 8000), (9000, 11448), (11000, 18000), (3000, 3400)]
    + [(rng.randint(0, 30000), rng.randint(0, 30000)) for _ in range(200)]
]
allotment_cases = [
    {"day_allow": d, "fwd_need": f, "expect": B.enrich_allotment(d, f)}
    for d, f in [(113000, 20000), (12000, 20000), (113000, 0), (113000, 112400), (113000, 112401)]
    + [(rng.randint(0, 120000), rng.randint(0, 120000)) for _ in range(200)]
]

json.dump(
    {
        "plan": PLAN,
        "seed": SEED,
        "limits": {k: P[k] for k in ("day_max", "month", "run", "pace_head", "need", "reserve_min", "enr_wall_margin")},
        "cases": cases,
        "forward_need_cases": forward_need_cases,
        "allotment_cases": allotment_cases,
    },
    sys.stdout,
    separators=(",", ":"),
)
