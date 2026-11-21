import { afterEach, describe, expect, it } from "vitest";
import { en, enPlurals } from "./en";
import { ru, ruPlurals } from "./ru";
import {
  formatCount,
  localizeBackend,
  pluralForm,
  setLang,
  t,
  tc,
} from "./index";

afterEach(() => setLang("en"));

const placeholders = (template: string) =>
  [...template.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();

describe("message catalogues", () => {
  it("has no empty translations", () => {
    for (const [key, value] of Object.entries(ru)) {
      expect(value.trim(), `ru:${key}`).not.toBe("");
    }
    for (const [key, value] of Object.entries(en)) {
      expect(value.trim(), `en:${key}`).not.toBe("");
    }
  });

  it("keeps the same placeholders in both languages", () => {
    for (const key of Object.keys(en) as (keyof typeof en)[]) {
      expect(placeholders(ru[key]), key).toEqual(placeholders(en[key]));
    }
    for (const key of Object.keys(enPlurals) as (keyof typeof enPlurals)[]) {
      const enForms = Object.values(enPlurals[key]);
      const ruForms = Object.values(ruPlurals[key]);
      expect(ruForms.length, key).toBeGreaterThanOrEqual(enForms.length);
      for (const form of [...enForms, ...ruForms]) {
        expect(placeholders(form), key).toEqual(["{n}".slice(1, 2)]);
      }
    }
  });

  it("actually translates the interface, leaving only brand names alone", () => {
    const identical = Object.keys(en).filter((key) => {
      const typed = key as keyof typeof en;
      return en[typed] === ru[typed];
    });
    // Brand, tech stack and the two language names stay the same on purpose.
    expect(identical.sort()).toEqual(
      [
        "app.brand",
        "app.tagline",
        // Pure placeholder templates: "{files} in {groups}" has nothing to translate.
        "disk.readyBody",
        "dup.groupsSummary",
        "settings.languageEn",
        "settings.languageRu",
        "settings.version",
      ].sort(),
    );
  });
});

describe("interpolation", () => {
  it("substitutes variables", () => {
    expect(t("scan.complete", { files: "a", groups: "b" })).toBe("Scan complete");
    expect(t("delete.toFree", { size: "1 KB" })).toBe("1 KB to free");
    setLang("ru");
    expect(t("delete.toFree", { size: "1 КБ" })).toBe("освободится 1 КБ");
  });

  it("leaves unknown placeholders untouched instead of printing undefined", () => {
    expect(t("delete.toFree", {})).toBe("{size} to free");
  });
});

describe("plural rules", () => {
  it("follows the Russian one/few/many rule", () => {
    const cases: [number, "one" | "few" | "many"][] = [
      [1, "one"],
      [2, "few"],
      [4, "few"],
      [5, "many"],
      [11, "many"],
      [14, "many"],
      [21, "one"],
      [22, "few"],
      [25, "many"],
      [101, "one"],
      [111, "many"],
      [0, "many"],
    ];
    for (const [count, expected] of cases) {
      expect(pluralForm("ru", count), `ru ${count}`).toBe(expected);
    }
  });

  it("falls back to singular/other for English", () => {
    expect(pluralForm("en", 1)).toBe("one");
    expect(pluralForm("en", 0)).toBe("other");
    expect(pluralForm("en", 5)).toBe("other");
  });

  it("renders complete phrases", () => {
    setLang("ru");
    expect(tc("group", 1)).toBe("1 группа");
    expect(tc("group", 2)).toBe("2 группы");
    expect(tc("group", 5)).toBe("5 групп");
    expect(tc("bit", 12)).toBe("12 бит");
    expect(tc("minuteAgo", 3)).toBe("3 минуты назад");
  });

  it("formats counts with the active locale", () => {
    expect(formatCount(1234)).toBe("1,234");
    setLang("ru");
    expect(formatCount(1234).replace(/\u00a0/g, " ")).toBe("1 234");
  });
});

describe("backend messages", () => {
  it("translates the messages the Rust side actually emits", () => {
    setLang("ru");
    expect(localizeBackend("No duplicates were found")).toBe("Дубликаты не найдены");
    expect(localizeBackend("refusing to delete the last copy of a file")).toBe(
      "отказ удалять последнюю копию файла",
    );
    expect(localizeBackend("invalid argument: select at least one folder to scan")).toBe(
      "недопустимый аргумент: выберите хотя бы одну папку для сканирования",
    );
    expect(
      localizeBackend(
        "scanning system directory '/proc' requires explicit permission — enable \"Include system directories\"",
      ),
    ).toContain("'/proc'");
  });

  it("passes unknown text through instead of hiding it", () => {
    setLang("ru");
    const osError = "I/O error: No such file or directory (os error 2)";
    expect(localizeBackend(osError)).toBe("ошибка ввода-вывода: No such file or directory (os error 2)");
    const unknown = "something the table has never seen";
    expect(localizeBackend(unknown)).toBe(unknown);
    expect(localizeBackend(null)).toBe("");
  });

  it("leaves backend text untouched in English", () => {
    expect(localizeBackend("No duplicates were found")).toBe("No duplicates were found");
  });
});
