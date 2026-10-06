#!/usr/bin/env python3
"""Generate skin property names from the pinned beatoraja sources.

Run from any directory with Python 3.10+: python scripts/generate_skin_property_names.py
Use --source PATH for another reference checkout and --check for a read-only check.
Normal Rust builds use the checked-in output and require neither Java nor .local.
The small parser deliberately rejects unknown expressions and changed enum counts.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess


REFERENCE_COMMIT = "d22ce10bc13e7ddb27805a3adc15bb03312e4c78"
ROOT = Path(__file__).resolve().parents[1]
SKIN = Path("src/bms/player/beatoraja/skin")
OUTPUT = ROOT / "crates/bmz-skin-document/src/property_names/generated.rs"
ENUMS = {
    "ValueType": ("Integer", "INTEGER_NAMES", 148),
    "IndexType": ("Integer", "INDEX_NAMES", 62),
    "RateType": ("Float", "RATE_NAMES", 31),
    "FloatType": ("Float", "FLOAT_NAMES", 29),
    "BooleanType": ("Boolean", "BOOLEAN_NAMES", 216),
    "StringType": ("String", "STRING_NAMES", 26),
}
# Count full enum declarations, including irUserName's public camelCase spelling
# and IndexType's cleartype / cleartype_target entries: 512 in total.
TOKEN = re.compile(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|[A-Za-z_$][\w$]*|\d+|[^\s]', re.S)
PAIRS = {"(": ")", "[": "]", "{": "}"}


def tokens(source: str) -> list[str]:
    return [t for t in TOKEN.findall(source) if not t.startswith(("//", "/*"))]


def split_top(items: list[str], delimiter: str = ",") -> list[list[str]]:
    result, current, stack = [], [], []
    for token in items:
        if token == delimiter and not stack:
            result.append(current)
            current = []
            continue
        if token in PAIRS:
            stack.append(PAIRS[token])
        elif token in PAIRS.values():
            if not stack or stack.pop() != token:
                raise ValueError("unbalanced expression")
        current.append(token)
    if stack:
        raise ValueError("unterminated expression")
    if current:
        result.append(current)
    return result


def enum_entries(source: list[str], name: str) -> list[tuple[str, list[list[str]]]]:
    positions = [i for i in range(len(source) - 2) if source[i:i + 3] == ["enum", name, "{"]]
    if len(positions) != 1:
        raise ValueError(f"expected one enum {name}")
    start = positions[0] + 3
    stack = []
    for end in range(start, len(source)):
        token = source[end]
        if token == ";" and not stack:
            break
        if token in PAIRS:
            stack.append(PAIRS[token])
        elif token in PAIRS.values():
            if not stack or stack.pop() != token:
                raise ValueError(f"unexpected end of enum {name}")
    else:
        raise ValueError(f"missing enum terminator: {name}")
    entries = []
    for entry in split_top(source[start:end]):
        if len(entry) < 4 or entry[1] != "(" or entry[-1] != ")":
            raise ValueError(f"unsupported enum entry: {name}: {entry[:8]}")
        entries.append((entry[0], split_top(entry[2:-1])))
    return entries


def call(expression: list[str], name: str) -> list[list[str]]:
    if expression[:2] != [name, "("] or expression[-1:] != [")"]:
        raise ValueError(f"expected {name}(...): {expression}")
    return split_top(expression[2:-1])


def string(expression: list[str]) -> str:
    if len(expression) != 1 or not expression[0].startswith('"'):
        raise ValueError(f"expected string literal: {expression}")
    return json.loads(expression[0])


def generate(source: Path) -> str:
    commit = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
    if commit != REFERENCE_COMMIT:
        raise ValueError(f"reference HEAD is {commit}, expected {REFERENCE_COMMIT}")
    paths = [SKIN / "SkinProperty.java"] + [SKIN / "property" / f"{family}PropertyFactory.java" for family in ("Integer", "Float", "Boolean", "String")]
    subprocess.run(["git", "-C", str(source), "diff", "--exit-code", "HEAD", "--", *map(str, paths)], check=True, capture_output=True)
    texts = {path.name: (source / path).read_text(encoding="utf-8") for path in paths}
    factories = {family: tokens(texts[f"{family}PropertyFactory.java"]) for family in ("Integer", "Float", "Boolean", "String")}
    constants = dict(re.findall(r"public\s+static\s+final\s+int\s+(\w+)\s*=\s*([^;]+);", texts["SkinProperty.java"]))

    def integer(expression: list[str], visited: frozenset[str] = frozenset()) -> int:
        compact = "".join(expression)
        if re.fullmatch(r"-?\d+", compact):
            return int(compact)
        if compact in constants and compact not in visited:
            return integer(tokens(constants[compact]), visited | {compact})
        raise ValueError(f"unresolved integer: {compact}")

    tables = {}
    for enum, (family, table, expected) in ENUMS.items():
        entries = enum_entries(factories[family], enum)
        if len(entries) != expected:
            raise ValueError(f"{enum}: expected {expected} entries, got {len(entries)}")
        tables[table] = [(name, integer(args[0])) for name, args in entries]

    def ids(expression: list[str]) -> list[int]:
        if expression[0] == "numberedIds":
            first, count = map(integer, call(expression, "numberedIds"))
            return list(range(first, first + count))
        if expression[:5] == ["new", "int", "[", "]", "{"] and expression[-1] == "}":
            return [integer(part) for part in split_top(expression[5:-1])]
        raise ValueError(f"unsupported ID array: {expression}")

    # Extract the names from the helper too, preserving public spelling mistakes.
    ir_match = re.search(r"String\[\]\s+irClearNames\(String suffix\)\s*\{\s*return new String\[\]\s*\{(.*?)\};\s*\}", texts["IntegerPropertyFactory.java"], re.S)
    if not ir_match:
        raise ValueError("missing irClearNames helper")
    ir_names = []
    for expression in split_top(tokens(ir_match[1])):
        if expression[1:] != ["+", "suffix"]:
            raise ValueError(f"unsupported IR name: {expression}")
        ir_names.append(string(expression[:1]))

    def names(expression: list[str]) -> list[str]:
        if expression[0] == "numberedNames":
            prefix, count = call(expression, "numberedNames")
            return [f"{string(prefix)}{index}" for index in range(1, integer(count) + 1)]
        if expression[0] == "irClearNames":
            suffix, = call(expression, "irClearNames")
            return [name + string(suffix) for name in ir_names]
        if expression[:5] == ["new", "String", "[", "]", "{"] and expression[-1] == "}":
            return [string(part) for part in split_top(expression[5:-1])]
        raise ValueError(f"unsupported name array: {expression}")

    for family, expected in [("Integer", 8), ("Float", 1)]:
        entries = enum_entries(factories[family], f"{family}PropertyPattern")
        if len(entries) != expected:
            raise ValueError(f"unexpected {family} pattern count")
        for _, args in entries:
            if family == "Integer":
                scope = "".join(args[0])
                table = {"PropertyScope.VALUE": "INTEGER_PATTERN_NAMES", "PropertyScope.IMAGE_INDEX": "INDEX_PATTERN_NAMES"}[scope]
                id_arg, name_arg = args[1:3]
            else:
                table = "FLOAT_PATTERN_NAMES"
                id_arg, name_arg = args
            values, labels = ids(id_arg), names(name_arg)
            if len(values) != len(labels):
                raise ValueError(f"unequal pattern arrays: {table}")
            tables.setdefault(table, []).extend(zip(labels, values))

    numbered = {}
    for family, expected in [("String", 11), ("Boolean", 2)]:
        entries = enum_entries(factories[family], f"{family}PropertyPattern")
        if len(entries) != expected:
            raise ValueError(f"unexpected {family} pattern count")
        patterns = []
        for _, args in entries:
            first, count = integer(args[0]), integer(args[1])
            if family == "Boolean":
                prefix_match = re.search(r'PREFIX\s*=\s*("[^"]+")', texts["BooleanPropertyFactory.java"])
                if not prefix_match:
                    raise ValueError("missing Boolean pattern prefix")
                patterns.append((json.loads(prefix_match[1]), string(args[2]), first, count, 1, -1))
            else:
                for expression in args[3:]:
                    if expression[0] != "new":
                        raise ValueError(f"unsupported String pattern: {expression}")
                    pattern = call(expression[1:], "NamePattern")
                    prefix, suffix = map(string, pattern[:2])
                    direction, offset = 1, -1
                    if len(pattern) == 3:
                        mapper = pattern[2]
                        if mapper[:3] != ["value", "-", ">"]:
                            raise ValueError(f"unsupported index mapper: {mapper}")
                        body = mapper[3:]
                        if len(body) == 3 and body[:2] == ["value", "-"]:
                            offset = -integer(body[2:])
                        elif len(body) == 3 and body[1:] == ["-", "value"]:
                            direction, offset = -1, integer(body[:1])
                        else:
                            raise ValueError(f"unsupported index mapper: {mapper}")
                    elif len(pattern) != 2:
                        raise ValueError(f"unsupported NamePattern: {pattern}")
                    patterns.append((prefix, suffix, first, count, direction, offset))
        numbered[family.upper() + "_PATTERNS"] = patterns

    # Names can collide across families; within one lookup tier they must not.
    for family in ("INTEGER", "INDEX", "FLOAT", "RATE", "STRING", "BOOLEAN"):
        seen = set()
        for table in (family + "_PATTERN_NAMES", family + "_NAMES"):
            for name, _ in tables.get(table, []):
                if name in seen:
                    raise ValueError(f"duplicate {family} name: {name}")
                seen.add(name)
        for prefix, suffix, _, count, direction, offset in numbered.get(family + "_PATTERNS", []):
            for index in range(count):
                name = f"{prefix}{(index - offset) * direction}{suffix}"
                if name in seen:
                    raise ValueError(f"duplicate {family} pattern name: {name}")
                seen.add(name)

    lines = [
        "// @generated by scripts/generate_skin_property_names.py; do not edit.",
        f"// beatoraja reference: {REFERENCE_COMMIT}",
        "// Sources: SkinProperty.java and property/{Integer,Float,Boolean,String}PropertyFactory.java.",
        "", "use super::NumberedProperty;", "",
    ]
    for table, entries in tables.items():
        lines.append(f"pub(super) const {table}: &[(&str, i32)] = &[")
        lines.extend(f"    ({json.dumps(name)}, {value})," for name, value in sorted(entries))
        lines.extend(["];", ""])
    for table, patterns in numbered.items():
        lines.append(f"pub(super) const {table}: &[NumberedProperty] = &[")
        for prefix, suffix, first, count, direction, offset in patterns:
            lines.append(f"    NumberedProperty::new({json.dumps(prefix)}, {json.dumps(suffix)}, {first}, {count}, {direction}, {offset}),")
        lines.extend(["];", ""])
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ROOT / ".local/beatoraja")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        generated = generate(args.source)
        if args.check:
            if OUTPUT.read_bytes() != generated.encode("utf-8"):
                raise ValueError(f"generated output differs: {OUTPUT}")
            print("Property name tables match the pinned upstream sources (512 fixed names).")
        else:
            OUTPUT.parent.mkdir(parents=True, exist_ok=True)
            OUTPUT.write_bytes(generated.encode("utf-8"))
            print(f"Generated {OUTPUT}")
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
