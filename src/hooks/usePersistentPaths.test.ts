import { beforeEach, describe, expect, it, vi } from "vitest";
import { addUniquePath, readStoredPaths } from "./usePersistentPaths";

const KEY = "test.paths";

describe("addUniquePath", () => {
  it("appends a new path", () => {
    expect(addUniquePath(["/a"], "/b")).toEqual(["/a", "/b"]);
  });

  it("ignores duplicates case-insensitively", () => {
    expect(addUniquePath(["/Media/Photos"], "/media/photos")).toEqual(["/Media/Photos"]);
    expect(addUniquePath(["C:\\Users\\Me"], "c:\\users\\me")).toEqual(["C:\\Users\\Me"]);
  });

  it("ignores empty and whitespace-only input", () => {
    expect(addUniquePath(["/a"], "")).toEqual(["/a"]);
    expect(addUniquePath(["/a"], "   ")).toEqual(["/a"]);
  });

  it("trims surrounding whitespace before comparing", () => {
    expect(addUniquePath([], "  /tmp/x  ")).toEqual(["/tmp/x"]);
    expect(addUniquePath(["/tmp/x"], "  /tmp/x  ")).toEqual(["/tmp/x"]);
  });

  it("does not mutate the input array", () => {
    const original = ["/a"];
    addUniquePath(original, "/b");
    expect(original).toEqual(["/a"]);
  });
});

describe("readStoredPaths", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.restoreAllMocks();
  });

  it("returns an empty list when nothing is stored", () => {
    expect(readStoredPaths(KEY)).toEqual([]);
  });

  it("returns the stored list", () => {
    window.localStorage.setItem(KEY, JSON.stringify(["/a", "/b"]));
    expect(readStoredPaths(KEY)).toEqual(["/a", "/b"]);
  });

  it("drops non-string entries instead of failing", () => {
    window.localStorage.setItem(KEY, JSON.stringify(["/a", 42, null, { x: 1 }, "/b"]));
    expect(readStoredPaths(KEY)).toEqual(["/a", "/b"]);
  });

  it("recovers from corrupted JSON", () => {
    window.localStorage.setItem(KEY, "{not json");
    expect(readStoredPaths(KEY)).toEqual([]);
  });

  it("recovers when the value is not an array", () => {
    window.localStorage.setItem(KEY, JSON.stringify({ a: 1 }));
    expect(readStoredPaths(KEY)).toEqual([]);
  });

  it("returns an empty list when storage throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("storage blocked");
    });
    expect(readStoredPaths(KEY)).toEqual([]);
  });
});
