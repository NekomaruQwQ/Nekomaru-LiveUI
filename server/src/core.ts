/**
 * Internal HTTP endpoints for capture workers.
 *
 * These are called by `live-capture --mode auto` to report metadata.
 */

import { Hono } from "hono";
import { setComputed, clearComputed } from "./strings";
import { createLogger } from "./log";

const log = createLogger("core");

const app = new Hono();

/**
 * POST /api/core/streamInfo/:streamId — capture switch metadata.
 *
 * Called by live-capture --mode auto on each window switch.  Updates
 * the computed strings that the frontend displays.
 */
app.post("/streamInfo/:streamId", async (c) => {
    const body = await c.req.json<{
        hwnd: string;
        title: string;
        file_description: string;
        mode: string | null;
    }>();

    // Update computed strings.
    setComputed("$captureInfo", body.file_description || body.title);
    setComputed("$captureMode", "auto");

    if (body.mode) {
        setComputed("$liveMode", body.mode);
    } else {
        clearComputed("$liveMode");
    }

    log.info(`streamInfo: ${body.file_description || body.title} (mode: ${body.mode ?? "none"})`);
    return c.json({ ok: true });
});

export default app;
