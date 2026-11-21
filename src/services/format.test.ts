import { describe, expect, it } from "vitest";
import {
  baseName,
  categoryLabel,
  formatBytes,
  formatDuration,
  formatNumber,
  parentPath,
  percent,
  shortPath,
} from "./format";
import { setLang, tc } from "../i18n";

/** `Intl` puts a non-breaking space in Russian numbers; compare on plain text. */
const plain = (value: string) => value.replace(/\u00a0/g, " ");

describe("formatBytes", () => {
  it("returns 0 B for zero, negative and non-finite input", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-100)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe("0 B");
  });

  it("keeps bytes as whole numbers", () => {
    expect(formatBytes(1)).toBe("1 B");
    expect(formatBytes(999)).toBe("999 B");
  });

  it("switches to binary units at 1024", () => {
    expect(formatBytes(1024)).toBe("1.00 KB");
    expect(formatBytes(1536)).toBe("1.50 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.00 MB");
    expect(formatBytes(1024 ** 3)).toBe("1.00 GB");
    expect(formatBytes(1024 ** 4)).toBe("1.00 TB");
  });

  it("drops decimals once the value is large enough", () => {
    expect(formatBytes(10 * 1024)).toBe("10.0 KB");
    expect(formatBytes(100 * 1024)).toBe("100 KB");
    expect(formatBytes(999 * 1024)).toBe("999 KB");
  });
});

describe("formatNumber", () => {
  it("groups thousands", () => {
    expect(formatNumber(1234567)).toBe("1,234,567");
    expect(formatNumber(999)).toBe("999");
  });

  it("rounds fractional values", () => {
    expect(formatNumber(12.4)).toBe("12");
    expect(formatNumber(12.6)).toBe("13");
  });
});

describe("formatDuration", () => {
  it("handles invalid input", () => {
    expect(formatDuration(-1)).toBe("0s");
    expect(formatDuration(Number.NaN)).toBe("0s");
  });

  it("formats seconds, minutes and hours", () => {
    expect(formatDuration(0)).toBe("0s");
    expect(formatDuration(59_999)).toBe("59s");
    expect(formatDuration(60_000)).toBe("1m 00s");
    expect(formatDuration(90_000)).toBe("1m 30s");
    expect(formatDuration(3_600_000)).toBe("1h 00m");
    expect(formatDuration(3_930_000)).toBe("1h 05m");
  });
});

describe("shortPath", () => {
  it("leaves short paths untouched", () => {
    expect(shortPath("/tmp/a.txt")).toBe("/tmp/a.txt");
  });

  it("elides the head but keeps the file name", () => {
    const long = "/very/deeply/nested/folder/structure/that/goes/on/and/on/for/ever/file.txt";
    const result = shortPath(long, 40);
    expect(result.startsWith("…/")).toBe(true);
    expect(result.endsWith("file.txt")).toBe(true);
    expect(result.length).toBeLessThan(long.length);
  });

  it("handles Windows separators", () => {
    const long = "C:\\Users\\Someone\\AppData\\Local\\Some\\Very\\Deep\\Tree\\payload.bin";
    const result = shortPath(long, 40);
    expect(result).toContain("payload.bin");
  });
});

describe("parentPath", () => {
  it("strips the last segment for both separators", () => {
    expect(parentPath("/a/b/c.txt")).toBe("/a/b");
    expect(parentPath("C:\\a\\b\\c.txt")).toBe("C:\\a\\b");
  });

  it("returns the input when there is no parent", () => {
    expect(parentPath("file.txt")).toBe("file.txt");
  });
});

describe("baseName", () => {
  it("returns the file name for both separators", () => {
    expect(baseName("/a/b/c.txt")).toBe("c.txt");
    expect(baseName("C:\\a\\b\\c.txt")).toBe("c.txt");
  });

  it("returns the whole string when there is no separator", () => {
    expect(baseName("file.txt")).toBe("file.txt");
  });
});

describe("percent", () => {
  it("returns 0 for a zero total instead of NaN", () => {
    expect(percent(10, 0)).toBe(0);
  });

  it("computes and clamps the ratio", () => {
    expect(percent(50, 100)).toBe(50);
    expect(percent(100, 100)).toBe(100);
  });
});

describe("count phrases", () => {
  it("keeps a single group singular", () => {
    // Regression: the Similar Photos header said "1 groups".
    expect(tc("group", 1)).toBe("1 group");
    expect(tc("group", 2)).toBe("2 groups");
    expect(tc("copy", 1)).toBe("1 copy");
    expect(tc("copy", 3)).toBe("3 copies");
  });

  it("formats large counts with separators", () => {
    expect(tc("file", 12345)).toBe("12,345 files");
  });

  it("uses Russian plural rules after switching the language", () => {
    setLang("ru");
    try {
      expect(tc("file", 1)).toBe("1 файл");
      expect(tc("file", 3)).toBe("3 файла");
      expect(tc("file", 5)).toBe("5 файлов");
      expect(plain(tc("file", 12_345))).toBe("12 345 файлов");
      expect(formatBytes(1536)).toBe("1,50 КБ");
      expect(categoryLabel("image")).toBe("Изображения");
    } finally {
      setLang("en");
    }
  });

  it("localises categories and falls back for unknown ones", () => {
    expect(categoryLabel("image")).toBe("Images");
    expect(categoryLabel("unknown-category")).toBe("unknown-category");
  });
});
