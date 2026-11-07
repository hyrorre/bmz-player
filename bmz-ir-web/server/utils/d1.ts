export const D1_BIND_LIMIT = 100

// D1 caps bound parameters per statement, including those outside the IN clause.
export async function inChunks<T>(
  values: string[],
  run: (chunk: string[]) => Promise<T[]>,
  chunkSize = 90,
): Promise<T[]> {
  const rows: T[] = []
  for (let offset = 0; offset < values.length; offset += chunkSize) {
    rows.push(...(await run(values.slice(offset, offset + chunkSize))))
  }
  return rows
}
