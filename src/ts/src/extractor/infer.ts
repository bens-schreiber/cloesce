import { Model, D1Column } from "../ast";

/**
 * Normalize a name by converting to lowercase and removing underscores.
 */
export function normalizeName(name: string): string {
  return name.toLowerCase().replace(/_/g, "");
}

interface DeferredOneToOneInference {
  modelName: string;
  propertyName: string;
  referencedModelName: string;
}

interface DeferredManyInference {
  modelName: string;
  propertyName: string;
  referencedModelName: string;
}

export enum InferenceBuilderError {
  MissingPrimaryKeys,
  MissingMatchingColumns,
  AmbiguousRelationship,
  IncompleteForeignKeys,
  IncorrectForeignKeyTarget,
}

export class InferenceBuilder {
  private oneToOne: DeferredOneToOneInference[] = [];
  private many: DeferredManyInference[] = [];

  addOneToOne(inference: DeferredOneToOneInference): void {
    this.oneToOne.push(inference);
  }

  addMany(inference: DeferredManyInference): void {
    this.many.push(inference);
  }

  build(
    models: Record<string, Model>,
  ): Record<string, InferenceBuilderError[]> {
    const oneToOneErrors = this.processOneToOnes(models);
    const manyErrors = this.processManyInferences(models);
    return { ...oneToOneErrors, ...manyErrors };
  }

  private processOneToOnes(
    models: Record<string, Model>,
  ): Record<string, InferenceBuilderError[]> {
    const errors: Record<string, InferenceBuilderError[]> = {};

    for (const inference of this.oneToOne) {
      const { modelName, propertyName, referencedModelName } = inference;

      const model = models[modelName];
      const referencedModel = models[referencedModelName];
      errors[modelName] ??= [];

      // Get the referenced model's primary keys from the extracted model
      const referencedPrimaryKeys = referencedModel.primary_key_columns.map(
        (col) => col.value.name,
      );

      if (referencedPrimaryKeys.length === 0) {
        console.warn(
          `Could not infer one-to-one relationship for ${modelName}.${propertyName}: ` +
            `Referenced model ${referencedModelName} has no primary keys.`,
        );
        errors[modelName].push(InferenceBuilderError.MissingPrimaryKeys);
        continue;
      }

      // Find all possible ways to form complete FK sets to the referenced model
      // Column names must start with the property name
      const normalizedPropName = normalizeName(propertyName);
      const allColumns = [...model.columns, ...model.primary_key_columns];

      // Map from PK name to columns that match it
      const columnsByPk = new Map<string, D1Column[]>();

      for (const col of allColumns) {
        const normalizedColName = normalizeName(col.value.name);

        // Column must start with the property name
        if (!normalizedColName.startsWith(normalizedPropName)) {
          continue;
        }

        for (const refPkName of referencedPrimaryKeys) {
          const normalizedRefPkName = normalizeName(refPkName);
          if (!normalizedColName.endsWith(normalizedRefPkName)) {
            continue;
          }

          if (!columnsByPk.has(refPkName)) {
            columnsByPk.set(refPkName, []);
          }
          columnsByPk.get(refPkName)!.push(col);
          break;
        }
      }

      // Check if all PKs have at least one matching column and none have multiple
      if (columnsByPk.size !== referencedPrimaryKeys.length) {
        console.warn(
          `Could not infer one-to-one relationship for ${modelName}.${propertyName}: ` +
            `Missing matching columns for all primary keys of ${referencedModelName}.`,
        );
        errors[modelName].push(InferenceBuilderError.MissingMatchingColumns);
        continue;
      }

      // Check if any PK has multiple matching columns (ambiguous)
      let isAmbiguous = false;
      for (const [_, cols] of columnsByPk) {
        if (cols.length > 1) {
          isAmbiguous = true;
          break;
        }
      }

      if (isAmbiguous) {
        console.warn(
          `Could not infer one-to-one relationship for ${modelName}.${propertyName}: ` +
            `Multiple possible column sets could form the relationship.`,
        );
        errors[modelName].push(InferenceBuilderError.AmbiguousRelationship);
        continue;
      }

      // Get the single column for each PK
      const matchingColumns: Array<{ column: D1Column; refPkName: string }> =
        [];
      for (const refPkName of referencedPrimaryKeys) {
        const col = columnsByPk.get(refPkName)![0];
        matchingColumns.push({ column: col, refPkName });
      }

      // Check if existing foreign keys cover all primary keys
      const existingFks = matchingColumns.filter(
        (mc) => mc.column.foreign_key_reference !== null,
      );

      // If some FKs exist, all must exist and target the correct model
      if (existingFks.length > 0) {
        if (existingFks.length !== referencedPrimaryKeys.length) {
          console.warn(
            `Could not infer one-to-one relationship for ${modelName}.${propertyName}: ` +
              `Some but not all foreign keys are defined. All must be defined or none.`,
          );
          errors[modelName].push(InferenceBuilderError.IncompleteForeignKeys);
          continue;
        }

        // Check all FKs target the correct model
        const allCorrect = existingFks.every(
          (mc) =>
            mc.column.foreign_key_reference?.model_name === referencedModelName,
        );
        if (!allCorrect) {
          console.warn(
            `Could not infer one-to-one relationship for ${modelName}.${propertyName}: ` +
              `Existing foreign keys do not all target ${referencedModelName}.`,
          );
          errors[modelName].push(
            InferenceBuilderError.IncorrectForeignKeyTarget,
          );
          continue;
        }
      }

      // At this point, we can infer the one-to-one relationship
      // Add/update foreign keys on matching columns
      for (const mc of matchingColumns) {
        if (!mc.column.foreign_key_reference) {
          mc.column.foreign_key_reference = {
            model_name: referencedModelName,
            column_name: mc.refPkName,
          };
        }
      }

      // Add navigation property if it doesn't already exist
      model.navigation_properties.push({
        var_name: propertyName,
        model_reference: referencedModelName,
        kind: {
          OneToOne: {
            key_columns: matchingColumns.map((mc) => mc.column.value.name),
          },
        },
      });
    }

    return errors;
  }

  private processManyInferences(
    models: Record<string, Model>,
  ): Record<string, InferenceBuilderError[]> {
    const errors: Record<string, InferenceBuilderError[]> = {};

    const ambiguousInferences = new Set<string>();
    for (const inference of this.many) {
      const { modelName, propertyName, referencedModelName } = inference;
      const backReferences = this.many.filter(
        (inf) =>
          inf.modelName === referencedModelName &&
          inf.referencedModelName === modelName,
      );

      if (backReferences.length > 1) {
        ambiguousInferences.add(`${modelName}.${propertyName}`);
        for (const backRef of backReferences) {
          ambiguousInferences.add(
            `${backRef.modelName}.${backRef.propertyName}`,
          );
        }
      }
    }

    // Track processed many-to-many relationships to avoid duplicates
    const processedManyToMany = new Set<string>();

    for (const inference of this.many) {
      const { modelName, propertyName, referencedModelName } = inference;
      const model = models[modelName];
      const referencedModel = models[referencedModelName];
      errors[modelName] ??= [];

      const inferenceKey = `${modelName}.${propertyName}`;

      // Skip if this inference is ambiguous
      if (ambiguousInferences.has(inferenceKey)) {
        console.warn(
          `Could not infer many-to-many relationship for ${modelName}.${propertyName}: ` +
            `Ambiguous relationship detected.`,
        );
        errors[modelName].push(InferenceBuilderError.AmbiguousRelationship);
        continue;
      }

      const backReferences = this.many.filter(
        (inf) =>
          inf.modelName === referencedModelName &&
          inf.referencedModelName === modelName,
      );

      // Must be a many to many if there is one back reference
      if (backReferences.length === 1) {
        // Create a normalized key for this M:M relationship (alphabetically ordered)
        const m2mKey = [modelName, referencedModelName].sort().join("-");

        // Skip if we've already processed this M:M relationship from the other side
        if (processedManyToMany.has(m2mKey)) {
          continue;
        }
        processedManyToMany.add(m2mKey);

        model.navigation_properties.push({
          var_name: propertyName,
          model_reference: referencedModelName,
          kind: "ManyToMany",
        });

        referencedModel.navigation_properties.push({
          var_name: backReferences[0].propertyName,
          model_reference: modelName,
          kind: "ManyToMany",
        });
        continue;
      }

      if (backReferences.length > 1) {
        // This should have been caught in first pass, but keeping for safety
        console.warn(
          `Could not infer many-to-many relationship for ${modelName}.${propertyName}: ` +
            `Referenced model ${referencedModelName} has multiple array properties pointing back to ${modelName} (${backReferences.length} found).`,
        );
        errors[modelName].push(InferenceBuilderError.AmbiguousRelationship);
        continue;
      }

      const sourcePrimaryKeys = model.primary_key_columns.map(
        (col) => col.value.name,
      );
      if (sourcePrimaryKeys.length === 0) {
        console.warn(
          `Could not infer one-to-many relationship for ${modelName}.${propertyName}: ` +
            `Source model ${modelName} has no primary keys.`,
        );
        errors[modelName].push(InferenceBuilderError.MissingPrimaryKeys);
        continue;
      }

      const oneToOneNavProps = referencedModel.navigation_properties.filter(
        (nav) =>
          nav.model_reference === modelName &&
          typeof nav.kind === "object" &&
          "OneToOne" in nav.kind,
      );

      const potentialFkColumns = [
        ...referencedModel.columns,
        ...referencedModel.primary_key_columns,
      ].filter((col) => col.foreign_key_reference?.model_name === modelName);

      const fkSets: string[][] = [];
      if (potentialFkColumns.length > 0) {
        const fkGroups = new Map<string, string[]>();

        for (const col of potentialFkColumns) {
          const normalizedColName = normalizeName(col.value.name);

          for (const sourcePk of sourcePrimaryKeys) {
            const normalizedSourcePk = normalizeName(sourcePk);
            if (!normalizedColName.endsWith(normalizedSourcePk)) {
              continue;
            }

            const prefix = normalizedColName.slice(
              0,
              -normalizedSourcePk.length,
            );
            if (!fkGroups.has(prefix)) {
              fkGroups.set(prefix, []);
            }
            fkGroups.get(prefix)!.push(col.value.name);
            break;
          }
        }

        for (const [_, fkColumns] of fkGroups) {
          if (fkColumns.length === sourcePrimaryKeys.length) {
            fkSets.push(fkColumns);
          }
        }
      }

      const oneToOneCount = oneToOneNavProps.length;
      const fkSetCount = fkSets.length;

      if (oneToOneCount === 1 && fkSetCount === 1) {
        const navPropColumns = (
          oneToOneNavProps[0].kind as { OneToOne: { key_columns: string[] } }
        ).OneToOne.key_columns;
        const normalizedNavCols = navPropColumns.map(normalizeName).sort();
        const normalizedFkCols = fkSets[0].map(normalizeName).sort();

        if (
          JSON.stringify(normalizedNavCols) === JSON.stringify(normalizedFkCols)
        ) {
          model.navigation_properties.push({
            var_name: propertyName,
            model_reference: referencedModelName,
            kind: { OneToMany: { key_columns: navPropColumns } },
          });
        }
        continue;
      }

      if (oneToOneCount === 1 && fkSetCount === 0) {
        const navPropColumns = (
          oneToOneNavProps[0].kind as { OneToOne: { key_columns: string[] } }
        ).OneToOne.key_columns;
        model.navigation_properties.push({
          var_name: propertyName,
          model_reference: referencedModelName,
          kind: { OneToMany: { key_columns: navPropColumns } },
        });
        continue;
      }

      if (oneToOneCount === 0 && fkSetCount === 1) {
        model.navigation_properties.push({
          var_name: propertyName,
          model_reference: referencedModelName,
          kind: { OneToMany: { key_columns: fkSets[0] } },
        });
        continue;
      }

      if (oneToOneCount === 0 && fkSetCount === 0) {
        console.warn(
          `Could not infer one-to-many relationship for ${modelName}.${propertyName}: ` +
            `Referenced model ${referencedModelName} has no foreign keys or 1:1 relationships back to ${modelName}.`,
        );
        errors[modelName].push(InferenceBuilderError.MissingMatchingColumns);
        continue;
      }

      // Multiple ways to form the relationship
      console.warn(
        `Could not infer one-to-many relationship for ${modelName}.${propertyName}: ` +
          `Referenced model ${referencedModelName} has multiple ways to reference ${modelName} ` +
          `(${oneToOneCount} 1:1 relationships, ${fkSetCount} FK sets).`,
      );
      errors[modelName].push(InferenceBuilderError.AmbiguousRelationship);
    }

    return errors;
  }
}
