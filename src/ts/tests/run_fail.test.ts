import fs from "fs";
import path from "path";
import { describe, test, expect } from "vitest";
import { Project } from "ts-morph";
import { CidlExtractor } from "../src/extractor/extract";
import { ExtractorError, ExtractorErrorCode } from "../src/extractor/err";

const FIXTURE_ROOT = path.resolve(__dirname, "fixtures");

describe("Extractor Run-Fail", () => {
  for (const file of files) {
    const lines = fs.readFileSync(file, "utf8").split(/\r?\n/);
    const expectedName = lines[1].replace("//", "").trim();

    test.concurrent(path.relative(FIXTURE_ROOT, file), () => {
      const project = new Project({ compilerOptions: { strict: true } });
      project.addSourceFileAtPath(file);

      const res = CidlExtractor.extract("proj", project);
      expect(res.isLeft()).toBe(true);

      const actualName = getErrorNameFromCode(
        (res.value as ExtractorError).code,
      );
      expect(
        actualName,
        `Expected "${expectedName}" but got "${actualName}"`,
      ).toStrictEqual(expectedName);
    });
  }
});

const files = fs
  .readdirSync(FIXTURE_ROOT, { withFileTypes: true })
  .flatMap((dir) =>
    dir.isDirectory()
      ? fs
          .readdirSync(path.join(FIXTURE_ROOT, dir.name))
          .filter((f) => f.endsWith(".cloesce.ts"))
          .map((f) => path.join(FIXTURE_ROOT, dir.name, f))
      : [],
  );

function getErrorNameFromCode(code: number) {
  return Object.entries(ExtractorErrorCode).find(([, v]) => v === code)?.[0];
}
