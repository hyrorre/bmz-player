use crate::chart::{player::NoteJudge, Judge, Mode, Note};

const PGREAT: usize = 0;
const GREAT: usize = 1;
const GOOD: usize = 2;
const BAD: usize = 3;
const POOR: usize = 4;

// BMSの各判定の範囲(+-ms)。PGREAT, GREAT, GOOD, BAD, POOR, KPOORの順
const BMS_JUDGE_TABLE: [i32; 5] = [20, 60, 160, 250, 1000];

// BMSのLN終端の各判定の範囲(+-ms)。PGREAT, GREAT, GOOD, BAD, POOR, KPOORの順
const BMS_LNEND_JUDGE_TABLE: [i32; 5] = [100, 150, 200, 250, 1000];

// BMSのスクラッチの各判定の範囲(+-ms)。PGREAT, GREAT, GOOD, BAD, POOR, KPOORの順
const SCR_JUDGE_TABLE: [i32; 5] = [30, 75, 200, 300, 1000];

// BMSのBSS終端の各判定の範囲(+-ms)。PGREAT, GREAT, GOOD, BAD, POOR, KPOORの順
const BSSEND_JUDGE_TABLE: [i32; 5] = [100, 150, 250, 300, 1000];

// PMSの各判定の範囲(+-ms)。PGREAT, GREAT, GOOD, BAD, POOR, KPOORの順
const PMS_JUDGE_TABLE: [i32; 5] = [25, 75, 175, 200, 1000];

// PMSのLN終端の各判定の範囲(+-ms)。PGREAT, GREAT, GOOD, BAD, POOR, KPOORの順
const PMS_LNEND_JUDGE_TABLE: [i32; 5] = [100, 150, 175, 200, 1000];

pub fn get_judge_table(lane: i32, is_lnend: bool, mode: Mode) -> [i32; 5] {
    if mode.is_bms() {
        if lane == 8 {
            // if scratch
            if is_lnend {
                return BSSEND_JUDGE_TABLE;
            } else {
                return SCR_JUDGE_TABLE;
            }
        }
        // if normal note
        else {
            if is_lnend {
                return BMS_LNEND_JUDGE_TABLE;
            } else {
                return BMS_JUDGE_TABLE;
            }
        }
    }
    // if pms
    else {
        if is_lnend {
            return PMS_LNEND_JUDGE_TABLE;
        } else {
            return PMS_JUDGE_TABLE;
        }
    }
}

pub fn update_lane(notes: &Vec<Note>, lane: i32, _key_state: i32, play_ms: u64, mode: Mode, _judges: &mut Vec<NoteJudge>) {
    let judge_table = get_judge_table(lane, false, mode);
    for note in notes.iter().filter(|note| note.lane == lane) {
        if note.judge == Judge::JUDGE_YET {
            let diff = (note.ms as i128 - play_ms as i128) as i32;
            // poor notes
            if judge_table[BAD] < -diff {
                
            }
        }
    }
}
