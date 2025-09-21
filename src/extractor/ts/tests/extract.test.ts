import { CidlExtractor } from "../src/extract.js";
import { Project } from "ts-morph";

test("actions snapshot", () => {
  const project = new Project({
    tsConfigFilePath: "../../test_fixtures/tsconfig.json",
  });
  project.addSourceFileAtPath("../../test_fixtures/models.cloesce.ts");

  let extractor = new CidlExtractor("snapshotProject", "0.0.2");
  let models = extractor.extract(project);

  expect(models).toMatchSnapshot();
});
