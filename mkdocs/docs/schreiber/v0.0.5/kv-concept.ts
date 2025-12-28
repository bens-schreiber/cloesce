// @ts-nocheck

// Base class for KV
class KValue<V> {
    key: string;
    raw: unknown;
    value: V; // No guarantees it is a V.
    metadata: unknown;
}

/**
 * The most basic KV model possible. It is apart of the KV namespace "namespace",
 * and its key is a constant "Config".
 * 
 * The type hint for the value is "unknown", which tells the client to expect any JSON
 * value, and tells the router to fetch the value as JSON.
 * 
 * To get this model, a generated CRUD method `Config.get()` would exist on the client. It would
 * take no parameters, because the key is constant ("Config").
 * 
 * The LIST CRUD method here could be implemented as well under `Config.list` KV supports a method of
 * NAMESPACE.list({ prefix }). Cloesce would automate this to be `namespace.list({prefix: "Config"})`.
 * Note that list returns only the keys. Cloesce would then have to query each key to get their value,
 * returning a list with at most one value in this case. 
 * 
 * The SAVE CRUD method would simply open the endpoint to put data into, under `Config.save(value).`
 * Since `Config` extends an `unknown`, value could be anything. It would again default to the key "Config".
 * 
 * The DELETE CRUD method is also valid here, deleting the value at the constant key.
 * 
 * Instance methods would hydrate with the value and metadata.
 */
@KV("namespace")
class Config extends KValue<unknown> {
    @POST
    method1() { ... }

    static method2() { ... }
}

/**
 * The earlier `Config` example is akin to a singleton. It's key is
 * constant, it just exists under the namespace with the key "Config".
 * 
 * However, KV is often used to express multiple of something. For example,
 * there could be many configs, or in this models case, many users.
 * 
 * To represent this, KeyParams are introduced. KeyParams must be a string, because KV
 * keys are required to be text.
 * 
 * Each model exists at the key "User/:id". All endpoints will now take an additional id param.
 */
@KV("namespace")
class User extends KValue<unknown> {
    @KeyParam
    id: string;

    //...methods
}

/**
 * Many KeyParams can be listed. The order of the KeyParams matters, as they
 * will be placed in the key in the order they are declared.
 */
@KV("namespace")
class User extends KValue<unknown> {
    @KeyParam
    firstname: string;

    @KeyParam
    lastname: string;

    // User/:firstname/:lastname

}

/**
 * KV values can be anything, from JSON to text to a large BLOB. Keys also can have
 * lifetimes (which we should support in the future). 
 * 
 * Because of this, it's a common pattern to extend the key based routing system to represent composition.
 * For instance, User/:id/ could contain profile, settings, avatar, session, etc etc.
 * 
 * Note that there is no guarantee that these keys even exist, it could always be undefined.
 * 
 * Cloesce will automate this process for the developer.
 */
@KV("namespace")
class User extends KValue<unknown> {
    @KeyParam
    id: string;

    profile: KValue<unknown> | undefined; // => User/:id/profile
    settings: KValue<unkown> | undefined; // => User/:id/settings

    //...methods
}

/**
 * It's possible that a User has multiple profiles, settings, etc. Logically, the key
 * would contain another id like User/:id/profile/:pid.
 * 
 * This means Cloesce needs some way to create a KV Model that composes other KV Models.
 * Unlike D1 models, a KV model composing another means that the composed model is not
 * seperate, but rather a field of the parent.
 * 
 * Essentially, if A composes B, B does not exist as the key "B" but rather "A/:id/B/:id"
 * If another KV Model C existed, C would not be able to compose B. This enforces a tree
 * structure rather than a graph.
 * 
 * In this case, generated CRUD methods for `Profile` would have to take all parent ids along with the
 * models id.
 */
@KV("namespace")
class Profile extends KValue<unknown> {
    @KeyParam
    id: string; // At least one key param is required since User.profiles is an array. Compiler will enforce this.

    //...methods
}

@KV("namespace")
class User extends KValue<unknown> {
    @KeyParam
    id: string;

    profiles: Profile[] | undefined; // => User/:id/profile/:pid
    settings: KValue<unkown> | undefined; // => User/:id/settings

    //...methods
}

/**
 * Overfetching values could be an issue. KV Models will need data sources just like D1 Models
 */
@KV("namespace")
class User extends KValue<unknown> {
    @KeyParam
    id: string;


    @DataSource
    static readonly default: IncludeTree<User> = {
        profiles: {
            otherField: {
                ...
            },
            anotherField: {
                ...
            }
        }
    }

    profiles: Profile[] | undefined; // => User/:id/profile/:pid
    settings: KValue<unkown> | undefined; // => User/:id/settings

    //...methods
}


/**
 * Custom keys could be useful too, allowing the use of any parameter but with a custom
 * key format and different namespace.
 * 
 * This may be for a future version.
 */
@KV("namespace")
class User extends KValue<unknown> {
    @KeyParam
    id: string;

    @CustomKey("other_namespace", "CustomKeyFormat/:id") // uses the "id" field above
    customField: string;

    profiles: Profile[] | undefined; // => User/:id/profile/:pid
    settings: KValue<unkown> | undefined; // => User/:id/settings

    //...methods
}