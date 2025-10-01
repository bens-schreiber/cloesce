import { CloesceAst, Model } from "../src/common.js";
import { CidlExtractor } from "../src/extract.js";
import { Project } from "ts-morph";

test("actions snapshot", () => {
  const project = new Project({
    tsConfigFilePath: "../../test_fixtures/tsconfig.json",
  });
  project.addSourceFileAtPath("../../test_fixtures/models.cloesce.ts");
  let extractor = new CidlExtractor("snapshotProject", "0.0.2");
  let res = extractor.extract(project);
  expect(res.ok).toBe(true);

  let ast = res.value as CloesceAst;
  for (const m of Object.values(ast.models)) {
    if (m) {
      m.source_path = "void for tests";
    }
  }
  ast.wrangler_env.source_path = "void for tests";

  expect(ast).toMatchSnapshot();
});
