import { describe, test, expect } from "vitest";
import { defineConfig } from "../src/config";
import { createAst, ModelBuilder } from "./builder";

class User {
  id!: number;
  email!: string;
  posts!: Post[];
}
class Post {
  id!: number;
  authorId!: number;
  title!: string;
  author!: User;
  comments!: Comment[];
  tags!: Tag[];
}
class Comment {
  id!: number;
  postId!: number;
  content!: string;
  post!: Post;
}
class Tag {
  id!: number;
  name!: string;
}
function testAst() {
  return createAst({
    models: [
      ModelBuilder.model("User")
        .idPk()
        .col("id", "Integer")
        .col("email", "Text")
        .build(),
      ModelBuilder.model("Post")
        .idPk()
        .col("id", "Integer")
        .col("authorId", "Integer")
        .col("title", "Text")
        .build(),
      ModelBuilder.model("Comment")
        .idPk()
        .col("id", "Integer")
        .col("postId", "Integer")
        .col("content", "Text")
        .build(),
      ModelBuilder.model("Tag")
        .idPk()
        .col("id", "Integer")
        .col("name", "Text")
        .build(),
    ],
  });
}

describe("Config Builder", () => {
  test("comprehensive builder test with all features", () => {
    // Arrange
    const config = defineConfig({
      srcPaths: ["./src", "./models"],
      projectName: "blog-app",
      outPath: "./generated",
      workersUrl: "http://localhost:8787",
      migrationsPath: "./db/migrations",
      truncateSourcePaths: false,
    });

    config
      .model(User, (builder) => {
        builder
          .primaryKey("id")
          .oneToMany("posts")
          .references(Post, "authorId");
      })
      .model(Post, (builder) => {
        builder
          .primaryKey("id")
          .foreignKey("authorId")
          .references(User, "id")
          .oneToOne("author")
          .references(User, "id")
          .oneToMany("comments")
          .references(Comment, "postId")
          .manyToMany("tags")
          .references(Tag, "id");
      })
      .model(Comment, (builder) => {
        builder
          .primaryKey("id")
          .foreignKey("postId")
          .references(Post, "id")
          .oneToOne("post")
          .references(Post, "id");
      })
      .rawAst((ast) => {
        ast.project_name = "custom-blog-app";
      });

    // Act
    const ast = testAst();
    const modifiers = config._getAstModifiers();
    modifiers.forEach((mod) => mod(ast));

    // Assert
    expect(modifiers).toHaveLength(4);
    expect(config.srcPaths).toEqual(["./src", "./models"]);
    expect(config.projectName).toBe("blog-app");
    expect(config.outPath).toBe("./generated");
    expect(config.workersUrl).toBe("http://localhost:8787");
    expect(config.migrationsPath).toBe("./db/migrations");
    expect(config.truncateSourcePaths).toBe(false);

    expect(ast.models.User.primary_key).toEqual({
      name: "id",
      cidl_type: "Integer",
    });
    expect(ast.models.User.navigation_properties).toHaveLength(1);
    expect(ast.models.User.navigation_properties[0]).toEqual({
      var_name: "posts",
      model_reference: "Post",
      kind: { OneToMany: { column_reference: "authorId" } },
    });

    expect(ast.models.Post.primary_key).toEqual({
      name: "id",
      cidl_type: "Integer",
    });
    expect(ast.models.Post.columns[1].foreign_key_reference).toBe("User");
    expect(ast.models.Post.navigation_properties).toHaveLength(3);
    expect(ast.models.Post.navigation_properties).toContainEqual({
      var_name: "author",
      model_reference: "User",
      kind: { OneToOne: { column_reference: "id" } },
    });
    expect(ast.models.Post.navigation_properties).toContainEqual({
      var_name: "comments",
      model_reference: "Comment",
      kind: { OneToMany: { column_reference: "postId" } },
    });
    expect(ast.models.Post.navigation_properties).toContainEqual({
      var_name: "tags",
      model_reference: "Tag",
      kind: "ManyToMany",
    });

    expect(ast.models.Comment.primary_key).toEqual({
      name: "id",
      cidl_type: "Integer",
    });
    expect(ast.models.Comment.columns[1].foreign_key_reference).toBe("Post");
    expect(ast.models.Comment.navigation_properties).toHaveLength(1);
    expect(ast.models.Comment.navigation_properties[0]).toEqual({
      var_name: "post",
      model_reference: "Post",
      kind: { OneToOne: { column_reference: "id" } },
    });

    expect(ast.project_name).toBe("custom-blog-app");
  });

  test("overwrites existing navigation properties", () => {
    // Arrange
    const config = defineConfig({ srcPaths: ["./src"] });
    const ast = testAst();

    ast.models.Post.navigation_properties.push({
      var_name: "author",
      model_reference: "OldUser",
      kind: { OneToOne: { column_reference: "oldId" } },
    });

    config.model(Post, (builder) => {
      builder.oneToOne("author").references(User, "id");
    });

    // Act
    const modifiers = config._getAstModifiers();
    modifiers.forEach((mod) => mod(ast));

    // Assert
    expect(ast.models.Post.navigation_properties).toHaveLength(1);
    expect(ast.models.Post.navigation_properties[0].model_reference).toBe(
      "User",
    );
  });

  test("applies unique constraints to columns", () => {
    // Arrange
    class ProfessorCourseRating {
      id!: number;
      professorId!: number;
      courseId!: number;
      name!: string;
    }

    const config = defineConfig({ srcPaths: ["./src"] });
    const ast = createAst({
      models: [
        ModelBuilder.model("ProfessorCourseRating")
          .idPk()
          .col("id", "Integer")
          .col("professorId", "Integer")
          .col("courseId", "Integer")
          .col("name", "Text")
          .build(),
      ],
    });

    config.model(ProfessorCourseRating, (builder) => {
      builder
        .unique("professorId", "courseId")
        .unique("name");
    });

    // Act
    const modifiers = config._getAstModifiers();
    modifiers.forEach((mod) => mod(ast));

    // Assert
    const model = ast.models.ProfessorCourseRating;

    const professorIdCol = model.columns.find((c) => c.value.name === "professorId");
    expect(professorIdCol).toBeDefined();
    expect(professorIdCol!.unique_ids).toEqual([0]);

    const courseIdCol = model.columns.find((c) => c.value.name === "courseId");
    expect(courseIdCol).toBeDefined();
    expect(courseIdCol!.unique_ids).toEqual([0]);

    const nameCol = model.columns.find((c) => c.value.name === "name");
    expect(nameCol).toBeDefined();
    expect(nameCol!.unique_ids).toEqual([1]);

    const idCol = model.columns.find((c) => c.value.name === "id");
    expect(idCol).toBeDefined();
    expect(idCol!.unique_ids).toEqual([]);
  });
});
