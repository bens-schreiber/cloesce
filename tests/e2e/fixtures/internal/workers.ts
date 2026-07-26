import { HttpResult } from "cloesce";
import { createApp, DeckOfManyThings, VoxMachina, type Api, type CfEnv } from "./backend.js";
import type { Card } from "./backend.js";

const CARDS: Card[] = [
  { name: "The Donjon", isCursed: true },
  { name: "The Fates", isCursed: false },
  { name: "The Euryale", isCursed: true },
  { name: "The Sun", isCursed: false },
];

const deckOfManyThings: Api.DeckOfManyThings.Of = {
  draw(drawnBy: string) {
    const card = CARDS[drawnBy.length % CARDS.length];
    return HttpResult.ok(200, card.isCursed);
  },
};

const voxMachina: Api.VoxMachina.Of = {};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp()
      .worker(env)
      .register(DeckOfManyThings, deckOfManyThings)
      .register(VoxMachina, voxMachina)
      .run(request);
  },
};
