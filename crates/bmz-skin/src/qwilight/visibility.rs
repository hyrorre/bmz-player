use super::*;

// Stable saved keys; labels can change without losing a user's selections.
const GROUPS: &[(&str, &str, &[i32])] = &[
    ("Title", "曲名", &[47]),
    ("Artist", "アーティスト", &[48]),
    ("Genre", "ジャンル", &[51]),
    ("NPS", "NPS", &[44]),
    ("Time", "経過時間・総時間", &[29]),
    ("Level", "譜面レベル", &[52]),
    ("Table", "難易度表のレベル", &[59]),
    ("Difficulty", "難易度のバッジ", &[60]),
    ("Timing", "打鍵タイミング", &[43]),
    ("Judgment graph", "判定推移グラフ", &[61]),
    ("Spectrum", "スペクトラムアナライザ", &[1]),
    ("BGA", "BGA", &[0]),
    ("Judgment", "判定文字", &[15]),
    ("Combo", "コンボ", &[9]),
    ("Timing ms", "FAST/SLOW のミリ秒", &[10]),
    ("FAST SLOW", "FAST/SLOW の累計", &[54, 55]),
    ("Judgment counts", "判定の内訳", &[35, 36, 37, 38, 39, 40]),
    ("RATE", "判定比率 (RATE)", &[56]),
    ("Score rate", "EX スコアレート", &[12]),
    ("HS", "ハイスピード", &[14]),
    ("BPM", "BPM", &[13]),
    ("Score", "EX スコア", &[11]),
    ("Gauge", "グルーブゲージ", &[6]),
    ("Bomb", "ノーツ処理のエフェクト", &[45]),
    ("Judgment line", "判定ライン", &[31]),
    ("Max combo", "最大コンボ", &[41]),
    ("Completion", "フルコン・クリア演出", &[62]),
    ("Gauge percent", "ゲージのパーセント", &[34]),
    ("Playback speed", "再生倍率", &[33]),
    ("Target difference", "目標スコアとの差", &[46]),
    ("Judgment ratio bar", "判定内訳の帯", &[30]),
    ("Auto play", "オートプレイ表示", &[4, 53]),
    ("Progress", "進行状況", &[28, 58]),
    ("Lane timing", "レーン別 FAST/SLOW", &[57]),
];

pub(super) fn properties(properties: &mut Vec<Json>, labels: &mut serde_json::Map<String, Json>) {
    properties.push(json!({"name":"Qwilight time display","def":"Elapsed","item":[{"name":"Elapsed","op":32500},{"name":"Remaining","op":32501}]}));
    labels.insert("Qwilight time display".into(),json!({"name":{"ja":"時間の表示"},"items":{"Elapsed":{"ja":"経過時間"},"Remaining":{"ja":"残り時間"}}}));
    for (index, (key, label, _)) in GROUPS.iter().enumerate() {
        let name = format!("Qwilight show {key}");
        properties.push(json!({"name":name,"def":"On","item":[
            {"name":"On","op":32400+index*2}, {"name":"Off","op":32401+index*2}
        ]}));
        labels.insert(
            name,
            json!({"name":{"ja":format!("表示: {label}")},
            "items":{"On":{"ja":"表示"},"Off":{"ja":"非表示"}}}),
        );
    }
}

pub(super) fn hidden(options: &BTreeMap<String, String>) -> Vec<i32> {
    GROUPS
        .iter()
        .filter(|(key, _, _)| {
            options.get(&format!("Qwilight show {key}")).is_some_and(|v| v == "Off")
        })
        .flat_map(|(_, _, pipelines)| pipelines.iter().copied())
        .collect()
}
