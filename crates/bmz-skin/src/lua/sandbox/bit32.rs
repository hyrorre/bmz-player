use super::*;

/// Lua 5.2 `bit32`, which LuaJ loads into every beatoraja / LR2oraja Endless
/// Dream skin VM but Lua 5.4 no longer ships.
///
/// Operands are reduced modulo 2^32 (non-integral numbers are truncated toward
/// zero first) and results are non-negative integers below 2^32. The library is
/// only installed when no global named `bit32` exists.
pub(super) fn install_bit32_library(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    if !matches!(globals.raw_get::<Value>("bit32")?, Value::Nil) {
        return Ok(());
    }
    let table = lua.create_table()?;
    for (name, initial, fold) in [
        ("band", u32::MAX, (|a, b| a & b) as fn(u32, u32) -> u32),
        ("bor", 0, |a, b| a | b),
        ("bxor", 0, |a, b| a ^ b),
    ] {
        table.set(
            name,
            lua.create_function(move |lua, args: Variadic<Value>| {
                fold_bit32_args(lua, args, initial, fold).map(i64::from)
            })?,
        )?;
    }
    table.set(
        "btest",
        lua.create_function(|lua, args: Variadic<Value>| {
            Ok(fold_bit32_args(lua, args, u32::MAX, |a, b| a & b)? != 0)
        })?,
    )?;
    table.set(
        "bnot",
        lua.create_function(|lua, value: Value| Ok(i64::from(!bit32_unsigned(lua, value)?)))?,
    )?;
    table.set(
        "lshift",
        lua.create_function(|lua, (value, displacement): (Value, Value)| {
            let value = bit32_unsigned(lua, value)?;
            Ok(i64::from(bit32_shift(value, bit32_integer(lua, displacement)?)))
        })?,
    )?;
    table.set(
        "rshift",
        lua.create_function(|lua, (value, displacement): (Value, Value)| {
            let value = bit32_unsigned(lua, value)?;
            let displacement = bit32_integer(lua, displacement)?;
            Ok(i64::from(bit32_shift(value, displacement.saturating_neg())))
        })?,
    )?;
    table.set(
        "arshift",
        lua.create_function(|lua, (value, displacement): (Value, Value)| {
            let value = bit32_unsigned(lua, value)?;
            let displacement = bit32_integer(lua, displacement)?;
            if displacement < 0 || value & 0x8000_0000 == 0 {
                return Ok(i64::from(bit32_shift(value, displacement.saturating_neg())));
            }
            Ok(i64::from(if displacement >= 32 {
                u32::MAX
            } else {
                ((value as i32) >> displacement) as u32
            }))
        })?,
    )?;
    for (name, direction) in [("lrotate", 1_i64), ("rrotate", -1)] {
        table.set(
            name,
            lua.create_function(move |lua, (value, displacement): (Value, Value)| {
                let value = bit32_unsigned(lua, value)?;
                let displacement = bit32_integer(lua, displacement)?.wrapping_mul(direction);
                Ok(i64::from(value.rotate_left(displacement.rem_euclid(32) as u32)))
            })?,
        )?;
    }
    table.set(
        "extract",
        lua.create_function(|lua, (value, field, width): (Value, Value, Option<Value>)| {
            let value = bit32_unsigned(lua, value)?;
            let (field, mask) = bit32_field(lua, field, width)?;
            Ok(i64::from((value >> field) & mask))
        })?,
    )?;
    table.set(
        "replace",
        lua.create_function(
            |lua, (value, replacement, field, width): (Value, Value, Value, Option<Value>)| {
                let value = bit32_unsigned(lua, value)?;
                let replacement = bit32_unsigned(lua, replacement)?;
                let (field, mask) = bit32_field(lua, field, width)?;
                Ok(i64::from((value & !(mask << field)) | ((replacement & mask) << field)))
            },
        )?,
    )?;
    globals.raw_set("bit32", table.clone())?;
    // LuaJ registers standard libraries in package.loaded as well as globals.
    // The sandboxed require uses this table before looking for skin files.
    let package: Table = globals.get("package")?;
    let loaded: Table = package.get("loaded")?;
    loaded.raw_set("bit32", table)
}

fn fold_bit32_args(
    lua: &Lua,
    args: Variadic<Value>,
    initial: u32,
    fold: fn(u32, u32) -> u32,
) -> mlua::Result<u32> {
    args.into_iter()
        .try_fold(initial, |result, value| Ok(fold(result, bit32_unsigned(lua, value)?)))
}

fn bit32_number(lua: &Lua, value: Value) -> mlua::Result<f64> {
    match value {
        Value::Integer(value) => Ok(value as f64),
        value => <f64 as mlua::FromLua>::from_lua(value, lua),
    }
}

fn bit32_unsigned(lua: &Lua, value: Value) -> mlua::Result<u32> {
    if let Value::Integer(value) = value {
        // Two's complement truncation is reduction modulo 2^32.
        return Ok(value as u32);
    }
    let value = bit32_number(lua, value)?;
    if !value.is_finite() {
        return Err(mlua::Error::runtime("bit32 operand must be a finite number"));
    }
    Ok(value.trunc().rem_euclid(4_294_967_296.0) as u32)
}

fn bit32_integer(lua: &Lua, value: Value) -> mlua::Result<i64> {
    if let Value::Integer(value) = value {
        return Ok(value);
    }
    let value = bit32_number(lua, value)?;
    if !value.is_finite() {
        return Err(mlua::Error::runtime("bit32 displacement must be a finite number"));
    }
    Ok(value.trunc() as i64)
}

/// Positive displacements shift left, negative ones shift right.
fn bit32_shift(value: u32, displacement: i64) -> u32 {
    match displacement {
        32.. | ..=-32 => 0,
        0.. => value << displacement,
        _ => value >> -displacement,
    }
}

fn bit32_field(lua: &Lua, field: Value, width: Option<Value>) -> mlua::Result<(u32, u32)> {
    let field = bit32_integer(lua, field)?;
    let width = match width {
        Some(Value::Nil) | None => 1,
        Some(width) => bit32_integer(lua, width)?,
    };
    if field < 0 {
        return Err(mlua::Error::runtime("bad argument to 'bit32' (field cannot be negative)"));
    }
    if width <= 0 {
        return Err(mlua::Error::runtime("bad argument to 'bit32' (width must be positive)"));
    }
    // Both inputs are positive here; bound the field before subtracting so
    // even math.maxinteger cannot overflow the range check itself.
    if field >= 32 || width > 32 - field {
        return Err(mlua::Error::runtime("trying to access non-existent bits"));
    }
    let mask = if width == 32 { u32::MAX } else { (1_u32 << width) - 1 };
    Ok((field as u32, mask))
}
