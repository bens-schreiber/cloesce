import { D1Database } from "@cloudflare/workers-types";
import { Integer, Model, WranglerEnv, Get, DataSource } from "cloesce/backend";

@WranglerEnv
export class Env {
    db: D1Database;
}

@Model(["SAVE"])
export class Topping {
    id: Integer;
    name: string;

    hamburgers: Hamburger[];
}

const onlyBaconDataSource: DataSource<Hamburger> = {
    includeTree: {
        toppings: {}
    },
    get: (joined) => `
        WITH cte AS (${joined()})
        SELECT * FROM CTE
        WHERE [toppings.name] = 'BACON'
        AND id = ?
        ORDER BY id
    `
};

@Model(["SAVE", "LIST"])
export class Hamburger {
    id: Integer;
    name: string;
    toppings: Topping[];

    static readonly orderedBurgersWithLettuce: DataSource<Hamburger> = {
        includeTree: {
            toppings: {}
        },
        list: (joined) => `
            WITH cte AS (${joined()})
            SELECT * FROM CTE
            WHERE [toppings.name] = 'LETTUCE'
            AND id > ?
            ORDER BY id
            LIMIT ?
        `,
        listParams: ["LastSeen", "Limit"]
    };

    @Get({
        includeTree: {
            toppings: {}
        },
        get: (joined) => `
            WITH cte AS (${joined()})
            SELECT * FROM CTE
            WHERE [toppings.name] != 'LETTUCE'
            AND id = ?
            ORDER BY id
        `
    })
    noLettuceToppings(): Topping[] {
        return this.toppings;
    }

    @Get(onlyBaconDataSource)
    onlyBacon(): Topping[] {
        return this.toppings;
    }
}

