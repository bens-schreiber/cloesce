import { HttpResult } from "cloesce";
import * as Cloesce from "./backend.js";


class Hamburger extends Cloesce.Hamburger.Api {
    noLettuceToppings(self: Cloesce.Hamburger.Self): Cloesce.Topping.Self[] {
        return self.toppings;
    }
    onlyBaconToppings(self: Cloesce.Hamburger.Self): Cloesce.Topping.Self[] {
        return self.toppings
    }
}

export default {
    async fetch(request: Request, env: Cloesce.Env): Promise<Response> {
        const app = await Cloesce.cloesce();
        app.register(new Hamburger());
        return app.run(request, env);
    }
}