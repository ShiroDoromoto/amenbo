// A byte count a reader can take in.
//
// Shared rather than written twice: an attachment's weight and a skin material's weight are the
// same question, and two answers to it would let one of them say `0.5 MB` while the other said
// `512 KB` on the same file.
import { formatNumber } from "./i18n";

/**
 * The largest unit the byte count fills, to one decimal while that decimal still says something.
 *
 * The digits go through `Intl` because the separator is the locale's — half a megabyte is `0.5 MB`
 * in English and `0,5 MB` in German.
 */
export function humanSize(n: bigint | number | null): string {
  if (n === null) return "";
  const bytes = Number(n);
  if (bytes < 1024) return `${formatNumber(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  const digits = v >= 10 ? 0 : 1;
  const size = formatNumber(v, { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return `${size} ${units[i]}`;
}
