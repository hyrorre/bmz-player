"""Classify residuals without changing replay inputs or fitting expected scores.

Time clusters only establish candidate correspondences: LR2 replay judgements do
not identify individual lanes/notes. Ambiguous clusters remain explicitly marked.
"""
import argparse
import collections
import csv
import hashlib
import json
from pathlib import Path
import subprocess


def assignment(cost):
    """Minimum cost square assignment, including explicit unmatched dummies."""
    n = len(cost)
    u, v, p, way = ([0] * (n + 1) for _ in range(4))
    for i in range(1, n + 1):
        p[0] = i
        j0 = 0
        minimum = [10**12] * (n + 1)
        used = [False] * (n + 1)
        while True:
            used[j0] = True
            i0, delta, j1 = p[j0], 10**12, 0
            for j in range(1, n + 1):
                if not used[j]:
                    current = cost[i0 - 1][j - 1] - u[i0] - v[j]
                    if current < minimum[j]:
                        minimum[j], way[j] = current, j0
                    if minimum[j] < delta:
                        delta, j1 = minimum[j], j
            for j in range(n + 1):
                if used[j]:
                    u[p[j]] += delta
                    v[j] -= delta
                else:
                    minimum[j] -= delta
            j0 = j1
            if p[j0] == 0:
                break
        while j0:
            p[j0] = p[way[j0]]
            j0 = way[j0]
    return {p[j] - 1: j - 1 for j in range(1, n + 1)}


def align(bmz, reference, tolerance=2, grade_cost=None):
    combined = sorted([(e['ms'], 0, e) for e in bmz] + [(e['ms'], 1, e) for e in reference],
                      key=lambda item: item[0])
    groups = []
    for item in combined:
        if not groups or item[0] - groups[-1][-1][0] > tolerance:
            groups.append([])
        groups[-1].append(item)
    for group in groups:
        left = [e for _, side, e in group if side == 0]
        right = [e for _, side, e in group if side == 1]
        n = len(left) + len(right)
        cost = [[0] * n for _ in range(n)]
        for i in range(n):
            for j in range(n):
                if i < len(left) and j < len(right):
                    difference = abs(left[i]['ms'] - right[j]['ms'])
                    mismatch_cost = grade_cost(left[i], right[j]) if grade_cost else 100
                    cost[i][j] = (difference + (0 if left[i]['grade'] == right[j]['grade'] else mismatch_cost)
                                  if difference <= tolerance else 1000000)
                elif i < len(left) or j < len(right):
                    cost[i][j] = 500
        chosen = assignment(cost)
        used = set()
        for i, event in enumerate(left):
            j = chosen[i]
            if j < len(right) and cost[i][j] < 1000:
                used.add(j)
                ambiguous = sum(abs(event['ms'] - r['ms']) <= tolerance for r in right) > 1
                yield event, right[j], ambiguous
            else:
                yield event, None, False
        for j, event in enumerate(right):
            if j not in used:
                yield None, event, False


def tap_grade(delta, windows):
    if delta > windows['bad_slow_us']:
        return 1
    for grade, field in [(5, 'pgreat_us'), (4, 'great_us'), (3, 'good_us')]:
        if abs(delta) <= windows[field]:
            return grade
    if -windows['bad_fast_us'] <= delta <= windows['bad_slow_us']:
        return 2
    if -windows['empty_poor_fast_us'] < delta < 0:
        return 0
    return None


def analyze(row, diagnostic):
    notes = {n['id']: n for n in diagnostic['notes']}
    left, right = [], []
    with Path(row['trace_path']).open(encoding='utf-8', newline='') as stream:
        for event in csv.DictReader(stream):
            if event['bmz_ms']:
                left.append({'ms': int(event['bmz_ms']), 'grade': int(event['bmz_judge']),
                             'id': int(event['bmz_note_id']) if event['bmz_note_id'] else None,
                             'delta_us': int(event['delta_us']), 'index': int(event['index'])})
            if event['reference_ms']:
                right.append({'ms': int(event['reference_ms']), 'grade': int(event['reference_judge']),
                              'index': int(event['index'])})
    residuals = []
    def grade_cost(event, expected):
        note = notes.get(event['id'])
        if note and note['kind'] == 'Tap':
            possible = {tap_grade(event['delta_us'] + shift * 1000, diagnostic['windows'])
                        for shift in range(-2, 3)}
            return 100 if expected['grade'] in possible else 300
        return 100

    for bmz, reference, ambiguous in align(left, right, grade_cost=grade_cost):
        if bmz and reference and bmz['grade'] == reference['grade']:
            continue
        note = notes.get(bmz['id']) if bmz else None
        category = 'unpaired_event'
        if bmz and reference:
            if note and note['kind'] == 'Tap':
                possible = {tap_grade(bmz['delta_us'] + shift * 1000, diagnostic['windows'])
                            for shift in (-1, 0, 1)}
                possible_two = {tap_grade(bmz['delta_us'] + shift * 1000, diagnostic['windows'])
                                for shift in (-2, 2)}
                category = ('tap_boundary_within_1ms' if reference['grade'] in possible
                            else 'tap_boundary_within_2ms' if reference['grade'] in possible_two
                            else 'tap_not_explained_by_2ms')
            elif note and note['kind'] == 'LongStart':
                category = 'long_note_requires_poll_history'
            else:
                category = 'unknown_note'
        residuals.append({'category': category, 'bmz': bmz, 'reference': reference,
                          'note': note, 'ambiguous_correspondence': ambiguous})
    # Identify the causal *candidate*, rather than comparing shifted grade indexes.
    # Actual LR2 idle polls and judgement lanes are absent from old replays.
    for event in residuals:
        bmz, note = event['bmz'], event['note']
        if event['category'] != 'unpaired_event' or not bmz or not note:
            continue
        near = [old for old in left if old['grade'] == 1 and old['id'] in notes
                and notes[old['id']]['lane'] == note['lane']
                and 0 < bmz['ms'] - old['ms'] <= 2]
        if not near:
            continue
        previous = max(near, key=lambda e: e['ms'])
        candidates = [other for other in residuals if other['category'] == 'unpaired_event'
                      and other['bmz'] is None and other['reference']['grade'] in (1, 2)
                      and bmz['ms'] <= other['reference']['ms']
                      <= note['time_us'] // 1000 + diagnostic['windows']['bad_slow_us'] // 1000 + 5]
        if len(candidates) == 1:
            event['category'] = candidates[0]['category'] = 'miss_poll_consumption_candidate'
            event['preceding_poor'] = previous
            event['later_reference'] = candidates[0]['reference']
    for event in residuals:
        bmz, note = event['bmz'], event['note']
        if event['category'] != 'unpaired_event' or not bmz or not note or note['kind'] != 'LongStart':
            continue
        head_press_ms = note['time_us'] // 1000 + bmz['delta_us'] // 1000
        prior_bad = [old for old in left if old['grade'] == 2 and old['ms'] == head_press_ms
                     and old['id'] in notes and notes[old['id']]['lane'] == note['lane']
                     and notes[old['id']]['kind'] == 'Tap']
        reference_bad = any(old['grade'] == 2 and abs(old['ms'] - head_press_ms) <= 2 for old in right)
        candidates = [other for other in residuals if other['category'] == 'unpaired_event'
                      and other['bmz'] is None and other['reference']['grade'] == bmz['grade']
                      and abs(other['reference']['ms'] - head_press_ms) <= 2]
        if prior_bad and reference_bad and len(candidates) == 1:
            # Source audit: LR2 ProcSinglenote recurses directly into itself at
            # Scene04_Play.cpp:902; BMZ redispatches through the LN-aware path.
            event['category'] = candidates[0]['category'] = 'bad_recursion_into_ln_confirmed'
            event['head_press_ms'] = head_press_ms
            event['preceding_bad'] = prior_bad[0]
            event['immediate_reference'] = candidates[0]['reference']
    counts = dict(collections.Counter(r['category'] for r in residuals))
    boundary = {'tap_boundary_within_1ms', 'tap_boundary_within_2ms'}
    if 'bad_recursion_into_ln_confirmed' in counts:
        assessment = 'implementation_path_difference_confirmed'
    elif counts and set(counts) <= boundary:
        assessment = 'boundary_compatible_not_proven'
    elif set(counts) <= boundary | {'miss_poll_consumption_candidate'}:
        assessment = 'poll_consumption_compatible_not_proven'
    elif set(counts) <= boundary | {'long_note_requires_poll_history'}:
        assessment = 'long_note_boundary_requires_measurement'
    else:
        assessment = 'requires_further_investigation'
    return {**{k: row[k] for k in ['md5', 'title', 'ex_delta', 'rank_percent', 'seed']},
            'categories': counts, 'residuals': residuals,
            'assessment': assessment,
            'implementation_difference_confirmed': True if 'bad_recursion_into_ln_confirmed' in counts else None}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--results', type=Path, default=Path('.local/legacy-parity-final/results.jsonl'))
    parser.add_argument('--runner', type=Path, default=Path('target/debug/examples/legacy_compare.exe'))
    parser.add_argument('--output', type=Path, default=Path('.local/legacy-mismatch-analysis'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    rows = [r for r in map(json.loads, args.results.read_text(encoding='utf-8').splitlines())
            if r['status'] == 'mismatch']
    reports = []
    worker = subprocess.Popen([str(args.runner.resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              text=True, encoding='utf-8', bufsize=1)
    try:
        for row in rows:
            request = {k: row[k] for k in ['md5', 'chart_path', 'replay_path']}
            request['analysis_only'] = True
            worker.stdin.write(json.dumps(request) + '\n')
            worker.stdin.flush()
            diagnostic = json.loads(worker.stdout.readline())
            if diagnostic['status'] != 'analysis_input':
                raise RuntimeError(diagnostic)
            (args.output / (row['md5'] + '-input.json')).write_text(json.dumps(diagnostic), encoding='utf-8')
            reports.append(analyze(row, diagnostic))
    finally:
        worker.stdin.close()
        worker.wait(timeout=30)
    (args.output / 'analysis.json').write_text(json.dumps(reports, ensure_ascii=False, indent=2), encoding='utf-8')
    summary = {'charts': len(reports), 'assessments': dict(collections.Counter(r['assessment'] for r in reports)),
               'residual_categories': dict(sum((collections.Counter(r['categories']) for r in reports), collections.Counter())),
               'runner_sha256': hashlib.sha256(args.runner.read_bytes()).hexdigest(),
               'scope': 'Diagnostic candidate alignment, not reconstruction of unrecorded OpenLR2 polling or proof of clock causation'}
    (args.output / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    labels = {'boundary_compatible_not_proven': '発生条件は説明済み：判定境界（元プレイの原因は未確定）',
              'poll_consumption_compatible_not_proven': '発生条件は再現済み：POORと入力消費順（元プレイの履歴は未確定）',
              'long_note_boundary_requires_measurement': 'LN判定境界・保持状態の追加計測が必要',
              'implementation_path_difference_confirmed': 'BAD連続判定からLNへ進む実装差あり',
              'requires_further_investigation': '複合差・連鎖の追加調査が必要'}
    report = ['# LEGACY 残る50件の不一致分析', '',
              '判定結果を配列番号ではなく時刻で対応づけ、同時刻の順序入替を分離した。'
              '候補対応には最大2msを許容するが、元入力・ノート時刻・スコアは一切変更しない。', '',
              '## 判断', '',
              'BADから次のLNへ再帰する処理にソースで確認できる実装差があり、2譜面の記録でも再現している。'
              'その他は未記録の判定時刻・ポーリング履歴の影響を排除できず、実装差の有無は未確定。'
              '実装差がある2譜面も、その差だけでEX差の全量を説明したとは扱わない。', '',
              '| 分類 | 譜面数 |', '|---|---:|']
    report.extend(f'| {labels[k]} | {v} |' for k, v in summary['assessments'].items())
    report.extend(['', '## 全50件', '', '| 譜面 | MD5 | EX差（BMZ−記録） | 判断 |', '|---|---|---:|---|'])
    report.extend(f"| {r['title'].replace('|', '/')} | {r['md5']} | {r['ex_delta']:+d} | {labels[r['assessment']]} |" for r in reports)
    report.extend(['', '## 証拠と限界', '',
                   '- 判定境界23件には、同時刻に複数ノートがあり個別対応が一意でないケースを含む。境界差に整合する候補があることを示す。',
                   '- More Selfishの1件は±1msでは足りず、2msなら判定境界を越える。時計差だけが原因とは断定しない。',
                   '- POORの1〜2ms前倒しに続く別ノートの成功と、記録側の後刻のBAD/POORを17譜面で確認。実際のポーリング履歴がないので原因候補。',
                   '- すいーつ・くえすと！と「大豆の国の☆ソイデレラ☆…」の譜面で、タップBADと次LNの開始が同じ入力。OpenLR2は即時確定、BMZは終点まで保持。',
                   '- 元ソース: Scene04_Play.cpp:901付近のProcSinglenote再帰。BMZ: judge/engine/mod.rsのprocess_pressからprocess_press_onceへ再ディスパッチする経路。',
                   '- 人形楽団の65100ms付近にはPOOR/BAD境界から後続ノートへ連鎖する差がある。単純な1件の判定置換では原因を評価できない。',
                   '- analysis.jsonに各残差・ノート・候補対応の曖昧さ・直前POOR/BADの根拠を保存。入力JSONも併存。', ''])
    (args.output / 'report.md').write_text('\n'.join(report), encoding='utf-8')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
