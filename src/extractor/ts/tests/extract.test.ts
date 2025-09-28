import { CidlSpec, Model } from "../src/common.js";
import { CidlExtractor } from "../src/extract.js";
import { Project } from "ts-morph";

test("actions snapshot", () => {
  const project = new Project({
    tsConfigFilePath: "../../test_fixtures/tsconfig.json",
  });
  project.addSourceFileAtPath("../../test_fixtures/models.cloesce.ts");
  let extractor = new CidlExtractor("snapshotProject", "0.0.2");
  let cidl = extractor.extract(project);
  expect(cidl.ok).toBe(true);
  for (const m of (cidl.value as CidlSpec).models) {
    if (m) {
      m.source_path = "void for tests";
    }
  }
  expect(cidl.value).toMatchSnapshot();
});
