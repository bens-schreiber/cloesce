import { describe, test, expect, vi, beforeEach } from "vitest";

const mockExecImpl = vi.fn();

vi.mock("child_process", () => ({
    exec: (...args: any[]) => mockExecImpl(...args),
}));

const { cloesce } = await import("../src/vite");

describe("cloesce vite plugin", () => {
    beforeEach(() => {
        mockExecImpl.mockReset();
        mockExecImpl.mockImplementation((_cmd: string, cb: Function) => {
            cb(null, "", "");
        });
    });

    test("plugin has correct name", () => {
        const plugin = cloesce();
        expect(plugin.name).toBe("cloesce-compile");
    });

    describe("configureServer", () => {
        test("adds default watchDir to server watcher", () => {
            const plugin = cloesce();
            const mockServer = { watcher: { add: vi.fn() } };
            (plugin as any).configureServer(mockServer);
            expect(mockServer.watcher.add).toHaveBeenCalledWith("src/data");
        });

        test("adds custom watchDirs to server watcher", () => {
            const plugin = cloesce({ watchDirs: ["custom/dir", "other/dir"] });
            const mockServer = { watcher: { add: vi.fn() } };
            (plugin as any).configureServer(mockServer);
            expect(mockServer.watcher.add).toHaveBeenCalledWith("custom/dir");
            expect(mockServer.watcher.add).toHaveBeenCalledWith("other/dir");
        });

        test("adds no watchers when watchDirs is empty", () => {
            const plugin = cloesce({ watchDirs: [] });
            const mockServer = { watcher: { add: vi.fn() } };
            (plugin as any).configureServer(mockServer);
            expect(mockServer.watcher.add).not.toHaveBeenCalled();
        });
    });

    describe("hotUpdate", () => {
        function makeServer() {
            return {
                config: {
                    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() },
                },
            };
        }

        test("triggers compile on any file when include is empty", async () => {
            const plugin = cloesce();
            await (plugin as any).hotUpdate({ file: "/any/file.ts", server: makeServer() });
            expect(mockExecImpl).toHaveBeenCalledOnce();
        });

        test("skips file not matching include pattern", async () => {
            const plugin = cloesce({ include: ["/src/data/"] });
            await (plugin as any).hotUpdate({ file: "/src/other/file.ts", server: makeServer() });
            expect(mockExecImpl).not.toHaveBeenCalled();
        });

        test("triggers compile for file matching include pattern", async () => {
            const plugin = cloesce({ include: ["/src/data/"] });
            await (plugin as any).hotUpdate({ file: "/src/data/schema.ts", server: makeServer() });
            expect(mockExecImpl).toHaveBeenCalledOnce();
        });

        test("skips file matching default exclude pattern (.generated)", async () => {
            const plugin = cloesce();
            await (plugin as any).hotUpdate({ file: "/project/.generated/client.ts", server: makeServer() });
            expect(mockExecImpl).not.toHaveBeenCalled();
        });

        test("skips file matching custom exclude pattern", async () => {
            const plugin = cloesce({ exclude: ["/dist/"] });
            await (plugin as any).hotUpdate({ file: "/project/dist/output.ts", server: makeServer() });
            expect(mockExecImpl).not.toHaveBeenCalled();
        });

        test("exclude takes priority over include", async () => {
            const plugin = cloesce({ include: ["/src/data/"], exclude: ["/src/data/.generated/"] });
            await (plugin as any).hotUpdate({ file: "/src/data/.generated/client.ts", server: makeServer() });
            expect(mockExecImpl).not.toHaveBeenCalled();
        });

        test("prevents concurrent compilations", async () => {
            let resolveFirst!: () => void;
            mockExecImpl.mockImplementation((_cmd: string, cb: Function) => {
                resolveFirst = () => cb(null, "", "");
            });

            const plugin = cloesce();
            const server = makeServer();
            const first = (plugin as any).hotUpdate({ file: "/file.ts", server });
            await (plugin as any).hotUpdate({ file: "/file.ts", server });

            expect(mockExecImpl).toHaveBeenCalledOnce();
            resolveFirst();
            await first;
        });

        test("resets isCompiling after error so next compile can run", async () => {
            const error = new Error("compile failed");
            mockExecImpl.mockImplementationOnce((_cmd: string, cb: Function) => cb(error));
            mockExecImpl.mockImplementationOnce((_cmd: string, cb: Function) => cb(null, "", ""));

            const plugin = cloesce();
            const server = makeServer();

            await (plugin as any).hotUpdate({ file: "/file.ts", server });
            await (plugin as any).hotUpdate({ file: "/file.ts", server });

            expect(mockExecImpl).toHaveBeenCalledTimes(2);
        });

        test("logs error when compile fails", async () => {
            const error = new Error("compile error");
            mockExecImpl.mockImplementation((_cmd: string, cb: Function) => cb(error));

            const plugin = cloesce();
            const server = makeServer();
            await (plugin as any).hotUpdate({ file: "/file.ts", server });

            expect(server.config.logger.error).toHaveBeenCalledWith(
                expect.stringContaining("compile error"),
                expect.anything(),
            );
        });
    });

    describe("buildStart", () => {
        function makePluginContext() {
            return { warn: vi.fn(), error: vi.fn() };
        }

        test("runs compile on build start", async () => {
            const plugin = cloesce();
            await (plugin as any).buildStart.call(makePluginContext());
            expect(mockExecImpl).toHaveBeenCalledOnce();
        });

        test("skips if a hotUpdate compile is already in progress", async () => {
            let resolveHot!: () => void;
            mockExecImpl.mockImplementation((_cmd: string, cb: Function) => {
                resolveHot = () => cb(null, "", "");
            });

            const plugin = cloesce();
            const server = {
                config: { logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() } },
            };
            const ctx = makePluginContext();

            const hotPromise = (plugin as any).hotUpdate({ file: "/file.ts", server });
            await (plugin as any).buildStart.call(ctx);

            expect(mockExecImpl).toHaveBeenCalledOnce();
            resolveHot();
            await hotPromise;
        });

        test("resets isCompiling after error so hotUpdate can run", async () => {
            const error = new Error("build fail");
            mockExecImpl.mockImplementationOnce((_cmd: string, cb: Function) => cb(error));
            mockExecImpl.mockImplementationOnce((_cmd: string, cb: Function) => cb(null, "", ""));

            const plugin = cloesce();
            const ctx = makePluginContext();
            const server = {
                config: { logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() } },
            };

            await (plugin as any).buildStart.call(ctx);
            await (plugin as any).hotUpdate({ file: "/file.ts", server });

            expect(mockExecImpl).toHaveBeenCalledTimes(2);
        });

        test("logs warning when compile fails", async () => {
            const error = new Error("build compile failed");
            mockExecImpl.mockImplementation((_cmd: string, cb: Function) => cb(error));

            const plugin = cloesce();
            const ctx = makePluginContext();
            await (plugin as any).buildStart.call(ctx);

            expect(ctx.warn).toHaveBeenCalledWith(expect.stringContaining("build compile failed"));
        });
    });
});
