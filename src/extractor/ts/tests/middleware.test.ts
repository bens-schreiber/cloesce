import { describe, test, expect } from "vitest";
import { CidlExtractor } from "../src/extract";
import { Project } from "ts-morph";
import { ExtractorErrorCode } from "../src/common";

describe("Middleware Extraction", () => {
  const createProject = (code: string) => {
    const project = new Project({
      compilerOptions: {
        strictNullChecks: true,
      },
      useInMemoryFileSystem: true,
    });

    project.createSourceFile("test.cloesce.ts", code);
    return project;
  };

  test("extracts middleware class with handle method", () => {
    const code = `
      @Middleware
      class AuthMiddleware {
        handle(request: Request): boolean {
          return true;
        }
      }

      @WranglerEnv
      class Env {}
    `;

    const project = createProject(code);
    const extractor = new CidlExtractor("TestProject", "v0.0.2");
    const result = extractor.extract(project);

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.middleware).not.toBeNull();
      expect(result.value.middleware?.class_name).toBe("AuthMiddleware");
      expect(result.value.middleware?.method.name).toBe("handle");
      expect(result.value.middleware?.method.parameters).toHaveLength(1);
      expect(result.value.middleware?.method.parameters[0].name).toBe(
        "request",
      );
    }
  });

  test("extracts middleware with @Inject parameter", () => {
    const code = `
      @Middleware
      class AuthMiddleware {
        handle(
          request: Request,
          @Inject env: WranglerEnv
        ): boolean {
          return true;
        }
      }

      @WranglerEnv
      class WranglerEnv {}
    `;

    const project = createProject(code);
    const extractor = new CidlExtractor("TestProject", "v0.0.2");
    const result = extractor.extract(project);

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.middleware).not.toBeNull();
      expect(result.value.middleware?.method.parameters).toHaveLength(2);

      const envParam = result.value.middleware?.method.parameters[1];
      expect(envParam?.name).toBe("env");
      expect(envParam?.cidl_type).toEqual({ Inject: "WranglerEnv" });
    }
  });

  test("extracts middleware with multiple parameters", () => {
    const code = `
      @Middleware
      class AuthMiddleware {
        handle(
          request: Request,
          token: string,
          userId: number
        ): boolean {
          return true;
        }
      }

      @WranglerEnv
      class Env {}
    `;

    const project = createProject(code);
    const extractor = new CidlExtractor("TestProject", "v0.0.2");
    const result = extractor.extract(project);

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.middleware).not.toBeNull();
      expect(result.value.middleware?.method.parameters).toHaveLength(3);

      expect(result.value.middleware?.method.parameters[0].name).toBe(
        "request",
      );
      expect(result.value.middleware?.method.parameters[1].name).toBe("token");
      expect(result.value.middleware?.method.parameters[1].cidl_type).toBe(
        "Text",
      );
      expect(result.value.middleware?.method.parameters[2].name).toBe("userId");
      expect(result.value.middleware?.method.parameters[2].cidl_type).toBe(
        "Integer",
      );
    }
  });

  test("returns null when no middleware is defined", () => {
    const code = `
      @D1
      class Horse {
        @PrimaryKey
        id: number;
      }

      @WranglerEnv
      class Env {}
    `;

    const project = createProject(code);
    const extractor = new CidlExtractor("TestProject", "v0.0.2");
    const result = extractor.extract(project);

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.middleware).toBeNull();
    }
  });

  test("errors when middleware class is missing handle method", () => {
    const code = `
      @Middleware
      class AuthMiddleware {
        validate(request: Request): boolean {
          return true;
        }
      }

      @WranglerEnv
      class Env {}
    `;

    const project = createProject(code);
    const extractor = new CidlExtractor("TestProject", "v0.0.2");
    const result = extractor.extract(project);

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.value.code).toBe(
        ExtractorErrorCode.MissingMiddlewareMethod,
      );
      expect(result.value.context).toContain("AuthMiddleware");
      expect(result.value.context).toContain("handle");
    }
  });

  test("errors when multiple middleware classes are defined", () => {
    const code = `
      @Middleware
      class AuthMiddleware {
        handle(request: Request): boolean {
          return true;
        }
      }

      @Middleware
      class LoggingMiddleware {
        handle(request: Request): boolean {
          return true;
        }
      }

      @WranglerEnv
      class Env {}
    `;

    const project = createProject(code);
    const extractor = new CidlExtractor("TestProject", "v0.0.2");
    const result = extractor.extract(project);

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.value.code).toBe(ExtractorErrorCode.TooManyMiddlewares);
      expect(result.value.context).toContain("AuthMiddleware");
      expect(result.value.context).toContain("LoggingMiddleware");
    }
  });

  test("middleware with nullable parameter types", () => {
    const code = `
      @Middleware
      class AuthMiddleware {
        handle(
          request: Request,
          token: string | null
        ): boolean {
          return true;
        }
      }

      @WranglerEnv
      class Env {}
    `;

    const project = createProject(code);
    const extractor = new CidlExtractor("TestProject", "v0.0.2");
    const result = extractor.extract(project);

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.middleware).not.toBeNull();

      const tokenParam = result.value.middleware?.method.parameters[1];
      expect(tokenParam?.name).toBe("token");
      expect(tokenParam?.cidl_type).toEqual({ Nullable: "Text" });
    }
  });

  test("middleware source path is extracted correctly", () => {
    const code = `
      @Middleware
      class AuthMiddleware {
        handle(request: Request): boolean {
          return true;
        }
      }

      @WranglerEnv
      class Env {}
    `;

    const project = createProject(code);
    const extractor = new CidlExtractor("TestProject", "v0.0.2");
    const result = extractor.extract(project);

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.middleware).not.toBeNull();
      expect(result.value.middleware?.source_path).toContain("test.cloesce.ts");
    }
  });

  test("middleware can have private helper methods", () => {
    const code = `
      @Middleware
      class AuthMiddleware {
        handle(request: Request): boolean {
          return this.isValid(request);
        }

        private isValid(request: Request): boolean {
          return true;
        }
      }

      @WranglerEnv
      class Env {}
    `;

    const project = createProject(code);
    const extractor = new CidlExtractor("TestProject", "v0.0.2");
    const result = extractor.extract(project);

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.value.middleware).not.toBeNull();
      expect(result.value.middleware?.class_name).toBe("AuthMiddleware");
      expect(result.value.middleware?.method.name).toBe("handle");
    }
  });
});
