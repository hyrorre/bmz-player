// Include after Engine.h in the OpenLR2 translation unit. Disabled unless the
// OPENLR2_PARITY_TRACE environment variable names an output CSV file.
#pragma once
#include <cstdio>
#include <cstdlib>

namespace openlr2_parity {
inline std::FILE* output = nullptr;
inline bool initialized = false;
inline unsigned long long serial = 0;
inline void close_output() { if (output) std::fclose(output); output = nullptr; }
inline std::FILE* file() {
    if (!initialized) {
        initialized = true;
        const char* path = std::getenv("OPENLR2_PARITY_TRACE");
        if (path && *path) {
            output = std::fopen(path, "wb");
            if (output) {
                static char buffer[1 << 20];
                std::setvbuf(output, buffer, _IOFBF, sizeof(buffer));
                std::fputs("kind,poll_id,judge_ms,lane,key_state,cursor_before,cursor_after,note_ms,tail_ms,active_before,active_after,judge,ex_before,ex_after,pg_ms,gr_ms,gd_ms,bad_ms,empty_poor_ms,md5,seed,key_mode,arrange,arrange_2p,fixed_key,fixed_key_2p,scratch_random,scratch_random_2p,assist,assist_2p,battle,dp_flip,pitch_on\n", output);
                std::atexit(close_output);
            }
        }
    }
    return output;
}
struct Poll;
inline thread_local Poll* current = nullptr;
struct Poll {
    game* owner;
    int lane, key, timing, player, before = 0, active = 0;
    double note_time = 0, tail_time = 0;
    long long score = 0;
    unsigned long long id = 0;
    bool enabled = false, emitted = false;
    Poll* parent = nullptr;
    Poll(game* g, int l, int k, int t, int p) : owner(g), lane(l), key(k), timing(t), player(p) {
        if (!file() || g->gameplay.replay.status == 2) return;
        enabled = true;
        parent = current;
        current = this;
        id = ++serial;
        const auto& column = g->gameplay.bmsobj_note[lane];
        before = column.note_count;
        if (column.notes && before < column.count) {
            const auto& note = column.notes[before];
            note_time = note.realTiming;
            tail_time = note.realTiming_ln;
            active = note.active;
        }
        score = static_cast<long long>(g->gameplay.player[player].exscore);
    }
    ~Poll() {
        if (!enabled) return;
        const auto& column = owner->gameplay.bmsobj_note[lane];
        const int after_active = column.notes && before < column.count ? column.notes[before].active : 0;
        if (emitted || key == 1 || key == 3 || before != column.note_count || active != after_active) {
            const auto& p = owner->gameplay.player[player];
            std::fprintf(file(), "P,%llu,%d,%d,%d,%d,%d,%.17g,%.17g,%d,%d,,%lld,%lld,%d,%d,%d,%d,%d,%s,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d\n",
                id,timing,lane,key,before,column.note_count,note_time,tail_time,active,after_active,
                score,static_cast<long long>(p.exscore),static_cast<int>(p.judgetime[5]),
                static_cast<int>(p.judgetime[4]),static_cast<int>(p.judgetime[3]),
                static_cast<int>(p.judgetime[2]),static_cast<int>(p.judgetime[1]), owner->sSelect.bmsList[owner->sSelect.cur_song].hash.body,
                owner->gameplay.randomseed, owner->gameplay.keymode, owner->config.play.random[0],
                owner->config.play.random[1], owner->config.play.randFix[0],owner->config.play.randFix[1],
                owner->config.play.randSC[0],owner->config.play.randSC[1],owner->config.play.assist[0],
                owner->config.play.assist[1],owner->config.play.battle,owner->config.play.dpFlip,owner->audio.param.pitch_on);
        }
        current = parent;
    }
};
inline void judgement(int judge, int lane) {
    if (!current || !current->enabled) return;
    current->emitted = true;
    std::fprintf(file(), "J,%llu,%d,%d,,,,,,,,%d,,,,,,,\n", current->id,current->timing,lane,judge);
}
} // namespace openlr2_parity
