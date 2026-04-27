import * as Cloesce from "./backend.js";

const Hamburger = Cloesce.Hamburger.impl({
  noLettuceToppings(self) {
    return self.toppings;
  },

  onlyBaconToppings(self) {
    return self.toppings;
  },
});

export default {
  async fetch(request: Request, env: Cloesce.Env): Promise<Response> {
    const app = await Cloesce.cloesce();
    app.register(Hamburger);
    return app.run(request, env);
  },
};
