// Headless OpenLR2 judgement oracle.
//
// Links the unmodified OpenLR2Lib (judgement, BMS loader, gauges) and drives
// ProcNoteOnTiming exactly as Scene04_Play's polling loop does, but on a
// deterministic 1 ms clock and with no window, audio or skin. Given a chart,
// play options and key edges, it prints every judgement, mine hit and the
// resulting score/gauge state, so the same input can be replayed through BMZ
// LEGACY and the two outputs compared line by line.
//
// Input (stdin, one directive per line):
//   chart <path>            BMS file (UTF-8 path)
//   keymode <5|7|10|14>     BMSMETA keymode
//   random <p1> [p2]        OPTION_RANDOM_* (0 off, 1 mirror, 2 random)
//   seed <n>                LR2 random seed (non-zero)
//   gauge <0..5>            GROOVE HARD DEATH EASY P-ATTACK G-ATTACK
//   randfix <p1> [p2]       fixed key for RANDOM (replay op 105/155)
//   randsc <p1> [p2]        scratch randomisation (replay op 106/156)
//   dpflip <0|1>            DP side flip (replay op 206)
//   end <ms>                last poll (default: last object + 3000 ms)
//   columns <0|1>           debug: print X lines when a column's IR counts change
//   e <ms> <lane> <1|0>     key edge (press/release) on OpenLR2 lane 0..19
// Edges must be sorted by time. Several edges of one lane in one poll are
// passed as separate calls in order, as BMZ does.
//
// Output (stdout, CSV):
//   H,<key>,<value>                      header (totals, judge widths, ...)
//   N,<lane>,<index>,<realTiming>,<realTiming_ln>,<mine>,<bmsTiming>,<wav>
//   J,<ms>,<lane>,<judge 0..5>           in ApplyJudgeNote order
//   M,<ms>,<lane>,<damage>               in ApplyJudgeMine order
//   S,<ms>,<hp0..hp5>,<ex>,<combo>,<maxcombo>,<jc0..jc5>,<note_current>
//   E,<ms>,<closed|end>
//   R,<field>,<values...>                after E: the play part of the
//   G,<graph>,<1000 samples>             CustomIR IRScoreV1
//   X,<ms>,<lane>,<15 IR counts>         with `columns 1`
#include "../LR2/Engine.h"
#include "../LR2/Scene04_Play.h"
#include "../LR2/LR2_bmsload.h"
#include "../LR2/LR2_replay.h"
#include "../LR2/LR2_statplay.h"

#include <cstdio>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <sstream>
#include <string>
#include <vector>

namespace {
struct Edge {
    int ms, lane, press;
};

void print_state(game& g, int ms) {
    const auto& p = g.gameplay.player[0];
    std::printf("S,%d", ms);
    for (int i = 0; i < 6; ++i) std::printf(",%.17g", p.HP[i]);
    std::printf(",%d,%d,%d", static_cast<int>(p.exscore), p.now_combo, p.max_combo);
    for (int i = 0; i < 6; ++i) std::printf(",%d", p.judgecount[i]);
    std::printf(",%d\n", p.note_current);
}

void print_stats(const char* name, const EXTENDEDPLAYERSTATS& s, unsigned total) {
    std::printf("R,%s,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u\n", name, s.epg, s.lpg, s.egr, s.lgr, s.egd,
                s.lgd, s.ebd, s.lbd, s.epr, s.lpr, s.cb, s.fast, s.slow, s.noteCount, total);
}

void print_stats_line(int ms, int lane, const EXTENDEDPLAYERSTATS& s) {
    std::printf("X,%d,%d,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u\n", ms, lane, s.epg, s.lpg, s.egr, s.lgr, s.egd,
                s.lgd, s.ebd, s.lbd, s.epr, s.lpr, s.cb, s.fast, s.slow, s.noteCount);
}

void print_samples(const char* name, const int* values) {
    std::printf("G,%s", name);
    for (int i = 0; i < 1000; ++i) std::printf(",%d", values[i]);
    std::printf("\n");
}

// The end of Scene04_Play and the result scene for a single song: gauges
// drop to 0 when notes are left, the graphs are filled to the end and the
// clear is decided. Then the fields IRScoreInternal copies from the play.
void print_submission(game& g) {
    auto& p = g.gameplay.player[PLAYER_1];
    if (p.totalnotes > p.note_current)
        for (int gauge = 0; gauge < 6; ++gauge) p.HP[gauge] = 0;
    LogGraphPlayerDataToEnd(&g.gameplay.statgraph[PLAYER_1], &p);
    CheckClear(&p, p.gaugeType, 0);
    print_stats("total", p.extendedStats, p.totalnotes);
    for (int lane = 0; lane < 20; ++lane) {
        char name[16];
        std::snprintf(name, sizeof name, "col%d", lane);
        print_stats(name, p.extendedColumnStats[lane], g.gameplay.bmsobj_note[lane].count);
    }
    std::printf("R,score,%d,%d,%d,%d,%.17g,%d,%d\n", p.max_combo, p.gaugeType, p.score, p.exscore, p.rate,
                p.clearType, p.totalnotes);
    std::printf("R,hp");
    for (int gauge = 0; gauge < 6; ++gauge) std::printf(",%.17g", p.HP[gauge]);
    std::printf("\nR,runtime,%.17g\n", g.gameplay.song_runtime);
    const auto& graph = g.gameplay.statgraph[PLAYER_1];
    for (int gauge = 0; gauge < 6; ++gauge) {
        char name[8];
        std::snprintf(name, sizeof name, "hp%d", gauge);
        print_samples(name, graph.hp[gauge].data());
    }
    print_samples("combo", graph.combo);
    print_samples("exscore", graph.exscore);
    if (g.gameplay.p1Score.judge_queue_len > 0) {
        const CSTR ghost = g.gameplay.p1Score.EncodeGhostData();
        std::printf("R,ghost,%s\n", ghost.body ? ghost.body : "");
    } else {
        std::printf("R,ghost,\n");
    }
}

int fail(const char* message) {
    std::fprintf(stderr, "openlr2_oracle: %s\n", message);
    return 2;
}
}  // namespace

int main() {
    std::string chart;
    int keymode = 0, random1 = 0, random2 = 0, seed = 0, gauge = 0, end_ms = -1;
    int randfix1 = 0, randfix2 = 0, randsc1 = 0, randsc2 = 0, dpflip = 0, dump_objects = 0, columns = 0;
    std::vector<Edge> edges;
    for (std::string line; std::getline(std::cin, line);) {
        std::istringstream in(line);
        std::string key;
        in >> key;
        if (key == "chart") {
            std::getline(in >> std::ws, chart);
        } else if (key == "keymode") {
            in >> keymode;
        } else if (key == "random") {
            in >> random1;
            if (!(in >> random2)) random2 = 0;
        } else if (key == "seed") {
            in >> seed;
        } else if (key == "randfix") {
            in >> randfix1;
            if (!(in >> randfix2)) randfix2 = 0;
        } else if (key == "randsc") {
            in >> randsc1;
            if (!(in >> randsc2)) randsc2 = 0;
        } else if (key == "dpflip") {
            in >> dpflip;
        } else if (key == "gauge") {
            in >> gauge;
        } else if (key == "columns") {
            in >> columns;
        } else if (key == "objects") {
            in >> dump_objects;
        } else if (key == "end") {
            in >> end_ms;
        } else if (key == "e") {
            Edge e{};
            in >> e.ms >> e.lane >> e.press;
            if (e.lane < 0 || e.lane >= 20) return fail("lane out of range");
            if (!edges.empty() && e.ms < edges.back().ms) return fail("edges must be sorted");
            edges.push_back(e);
        } else if (!key.empty() && key[0] != '#') {
            return fail("unknown directive");
        }
    }
    if (chart.empty() || keymode == 0 || seed == 0) return fail("chart, keymode and seed are required");

    auto g = std::make_unique<game>();
    g->gameplay.courseType = -1;
    g->gameplay.isCourse = 0;
    g->gameplay.isAutoplay = 0;
    g->config.play.random[PLAYER_1] = random1;
    g->config.play.random[PLAYER_2] = random2;
    g->config.play.gaugeType[PLAYER_1] = gauge;
    g->config.play.gaugeType[PLAYER_2] = gauge;
    g->config.play.randFix[PLAYER_1] = randfix1;
    g->config.play.randFix[PLAYER_2] = randfix2;
    g->config.play.randSC[PLAYER_1] = randsc1;
    g->config.play.randSC[PLAYER_2] = randsc2;
    g->config.play.dpFlip = dpflip;
    g->config.play.battle = OPTION_BATTLE_OFF;
    InitGameplay(&g->gameplay, &g->config.play);
    g->gameplay.randomseed = seed;

    BMSMETA meta{};
    meta.keymode = keymode;
    CSTR path;
    path.assign(chart.c_str());
    ParseBmsFile(&g->gameplay, path, &g->audio, &g->config, &meta, 0, 0);

    // Record judgements through the normal replay hooks in ApplyJudgeNote /
    // ApplyJudgeMine; status 1 is "recording".
    AllocReplayBuffer(&g->gameplay.replay);
    g->gameplay.replay.status = 1;
    InitTimer(&g->timer1);
    SetManualTimerFlag(&g->timer1, 1);
    SetManualTimer(&g->timer1, 0);
    SetTimeLapse(41, &g->timer1);

    const auto& p = g->gameplay.player[0];
    std::printf("H,keymode,%d\nH,seed,%d\nH,randomseed,%d\nH,totalnotes,%d\nH,gaugeType,%d\n", keymode, seed,
                g->gameplay.randomseed, p.totalnotes, p.gaugeType);
    for (int i = 1; i <= 5; ++i) std::printf("H,judgetime%d,%d\n", i, p.judgetime[i]);
    for (int gtype = 0; gtype < 6; ++gtype)
        for (int j = 0; j < 6; ++j) std::printf("H,damage%d_%d,%.17g\n", gtype, j, p.judge_damage[gtype][j]);

    // Debug aid: every loaded object in time order (op after conversion).
    if (dump_objects) {
        for (int i = 0; i < g->gameplay.bmsobj.count; ++i) {
            const auto& n = g->gameplay.bmsobj.notes[i];
            std::printf("O,%d,%d,%.17g,%.17g\n", i, n.op, n.realTiming, n.val);
        }
    }

    double last = 0;
    for (int lane = 0; lane < 20; ++lane) {
        const auto& column = g->gameplay.bmsobj_note[lane];
        for (int i = 0; i < column.count; ++i) {
            const auto& n = column.notes[i];
            std::printf("N,%d,%d,%.17g,%.17g,%d,%.17g,%d\n", lane, i, n.realTiming, n.realTiming_ln, n.mine,
                        n.bmsTiming, static_cast<int>(n.val));
            if (n.realTiming > last) last = n.realTiming;
            if (n.realTiming_ln > last) last = n.realTiming_ln;
        }
    }
    for (int i = 0; i < g->gameplay.bmsobj_line.count; ++i) {
        const auto& n = g->gameplay.bmsobj_line.notes[i];
        std::printf("L,%d,%.17g\n", i, n.realTiming);
    }
    if (end_ms < 0) end_ms = static_cast<int>(last) + 3000;
    print_state(*g, -1);

    bool held[20]{};
    size_t cursor = 0;
    int emitted = 0;
    const char* reason = "end";
    int ms = 0;
    for (; ms <= end_ms; ++ms) {
        // Scene04_Play checks the gauge before each lane poll.
        const int gt = g->gameplay.player[PLAYER_1].gaugeType;
        if (g->gameplay.player[PLAYER_1].HP[gt] < 2.0) {
            reason = "closed";
            break;
        }
        SetManualTimer(&g->timer1, ms);
        const size_t start = cursor;
        while (cursor < edges.size() && edges[cursor].ms <= ms) ++cursor;
        const int before = g->gameplay.replay.count;
        for (int lane = 0; lane < 20; ++lane) {
            bool any = false;
            for (size_t i = start; i < cursor; ++i) {
                if (edges[i].lane != lane) continue;
                any = true;
                held[lane] = edges[i].press != 0;
                const int count = g->gameplay.replay.count;
                ProcNoteOnTiming(g.get(), lane, held[lane] ? 1 : 3, ms, 0);
                for (int r = count; r < g->gameplay.replay.count; ++r) {
                    const auto& d = g->gameplay.replay.data[r];
                    if (d.op == 210 || d.op == 211) std::printf("J,%d,%d,%d\n", ms, lane, d.value);
                    else if (d.op == 214 || d.op == 215) std::printf("M,%d,%d,%d\n", ms, lane, d.value);
                }
            }
            if (!any) {
                const int count = g->gameplay.replay.count;
                ProcNoteOnTiming(g.get(), lane, held[lane] ? 2 : 0, ms, 0);
                for (int r = count; r < g->gameplay.replay.count; ++r) {
                    const auto& d = g->gameplay.replay.data[r];
                    if (d.op == 210 || d.op == 211) std::printf("J,%d,%d,%d\n", ms, lane, d.value);
                    else if (d.op == 214 || d.op == 215) std::printf("M,%d,%d,%d\n", ms, lane, d.value);
                }
            }
        }
        if (g->gameplay.replay.count != before) {
            print_state(*g, ms);
            ++emitted;
        }
        if (columns) {
            static unsigned last[20][2];
            for (int lane = 0; lane < 20; ++lane) {
                const auto& c = g->gameplay.player[PLAYER_1].extendedColumnStats[lane];
                const unsigned sum[2] = {c.epg + c.lpg + c.egr + c.lgr + c.egd + c.lgd + c.ebd + c.lbd + c.epr + c.lpr,
                                         c.fast + c.slow};
                if (sum[0] == last[lane][0] && sum[1] == last[lane][1]) continue;
                last[lane][0] = sum[0];
                last[lane][1] = sum[1];
                print_stats_line(ms, lane, c);
            }
        }
        // Scene04_Play logs the graphs after the lane polls of each loop.
        LogGraphPlayData(&g->gameplay.statgraph[PLAYER_1], &g->gameplay.player[PLAYER_1], ms,
                         static_cast<int>(g->gameplay.song_runtime));
    }
    std::printf("E,%d,%s\n", ms, reason);
    print_submission(*g);
    return 0;
}
