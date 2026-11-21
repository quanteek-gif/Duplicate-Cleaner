import { describe, expect, it } from "vitest";
import { shortenPath } from "./formatPath";

describe("shortenPath", () => {
  it("leaves short paths untouched", () => {
    expect(shortenPath("C:\\Docs\\a.txt")).toBe("C:\\Docs\\a.txt");
  });

  it("keeps the file name visible when cutting", () => {
    const long = "/tmp/some/deeply/nested/folder/structure/that/goes/on/report.txt";
    const short = shortenPath(long, 30);
    expect(short.startsWith("…")).toBe(true);
    expect(short.endsWith("report.txt")).toBe(true);
    expect(short.length).toBeLessThanOrEqual(30);
  });

  it("cuts from the front and preserves the rest verbatim", () => {
    const path = "/tmp/sweep2/archive/final_report.pdf";
    const short = shortenPath(path, 20);
    expect(short.length).toBe(20);
    // Everything after the ellipsis must be the real tail of the path, in order.
    expect(short.slice(1)).toBe(path.slice(path.length - 19));
    expect(short.endsWith("final_report.pdf")).toBe(true);
  });
});
