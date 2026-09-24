import { describe, expect, it } from 'vitest';
import { fromLocalInput, toLocalInput } from './dates';

describe('publication dates', () => {
  it('round-trip through the input in local time', () => {
    const rfc = fromLocalInput('2026-09-17T10:00');
    expect(rfc).toMatch(/^2026-09-17T10:00:00[+-]\d\d:\d\d$/);
    expect(toLocalInput(rfc)).toBe('2026-09-17T10:00');
  });

  it('name the same instant the offset says', () => {
    const rfc = fromLocalInput('2026-09-17T10:00')!;
    expect(new Date(rfc).getTime()).toBe(new Date('2026-09-17T10:00').getTime());
  });

  it('treat empty as unset', () => {
    expect(fromLocalInput('')).toBeNull();
    expect(toLocalInput(null)).toBe('');
  });
});
