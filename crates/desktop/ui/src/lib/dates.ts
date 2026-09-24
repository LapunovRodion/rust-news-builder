/**
 * `<input type="datetime-local">` speaks local wall-clock time with no offset; the Rust side
 * speaks RFC 3339. These two convert between them in the editor's own time zone.
 */

const pad = (n: number) => String(n).padStart(2, '0');

/** `2026-09-17T10:00` (local) → `2026-09-17T10:00:00+03:00`. Empty stays null. */
export function fromLocalInput(value: string): string | null {
  if (!value) return null;
  const local = new Date(value);
  if (Number.isNaN(local.getTime())) return null;
  const minutes = -local.getTimezoneOffset();
  const sign = minutes >= 0 ? '+' : '-';
  const abs = Math.abs(minutes);
  const seconds = value.length === 16 ? `${value}:00` : value;
  return `${seconds}${sign}${pad(Math.floor(abs / 60))}:${pad(abs % 60)}`;
}

/** RFC 3339 → the local `YYYY-MM-DDTHH:MM` the input shows. Null stays empty. */
export function toLocalInput(value: string | null): string {
  if (!value) return '';
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return '';
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
