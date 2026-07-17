import * as clo from "@cloesce/backend.js";
import { HttpResult } from "cloesce";
import { auth } from "./auth.js";
import { User } from "./user.js";

export const SubReddit = clo.SubReddit.impl({
    async create(env, title, description) {
        const username = auth(env);
        if (username instanceof HttpResult) return username;

        const sub = await this.Default.save(env, { title, description, posts: [] });
        const id = sub.data!.id;

        await User.Default.save(env, username, { authoredSubReddits: [{ subRedditId: id }] });

        return { id, title, description, lastPostId: 0, posts: [] };
    },

    async feed(self) {
        return self.posts.map((link) => link.post);
    },
});
