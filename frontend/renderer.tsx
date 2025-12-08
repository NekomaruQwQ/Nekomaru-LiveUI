import { useRef, useEffect } from 'preact/hooks';
import { css } from '@emotion/css';
import { H264Decoder, parseStreamFrame } from './decoder';

/**
 * Video renderer component that decodes and displays H.264 stream
 */
export function VideoRenderer() {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const decoderRef = useRef<H264Decoder | null>(null);

    useEffect(() => {
        const canvas = canvasRef.current;
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        if (!ctx) {
            console.error('Failed to get 2D context');
            return;
        }

        // Create decoder
        const decoder = new H264Decoder((frame: VideoFrame) => {
            renderFrame(canvas, ctx, frame);
        });

        decoderRef.current = decoder;

        // Initialize and start stream loop
        decoder.init()
            .then(() => {
                console.log('Decoder initialized, starting stream');
                startStreamLoop(decoder);
            })
            .catch((e) => {
                console.error('Failed to initialize decoder:', e);
            });

        return () => {
            decoder.close();
        };
    }, []);

    return (
        <canvas
            ref={canvasRef}
            className={css({
                width: '100%',
                height: '100%',
                objectFit: 'contain',
                backgroundColor: '#000',
            })}
        />
    );
}

/**
 * Render a decoded video frame to canvas
 */
function renderFrame(canvas: HTMLCanvasElement, ctx: CanvasRenderingContext2D, frame: VideoFrame) {
    // Resize canvas if needed
    if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
        canvas.width = frame.displayWidth;
        canvas.height = frame.displayHeight;
        console.log(`Canvas resized to ${frame.displayWidth}x${frame.displayHeight}`);
    }

    // Draw frame
    ctx.drawImage(frame, 0, 0);

    // CRITICAL: Close frame to release GPU memory
    frame.close();
}

/**
 * Stream loop that fetches and decodes frames
 */
async function startStreamLoop(decoder: H264Decoder) {
    let lastSequence = 0;
    let consecutiveErrors = 0;
    const MAX_CONSECUTIVE_ERRORS = 10;

    while (true) {
        try {
            const response = await fetch(`stream://stream?after=${lastSequence}`);

            if (!response.ok) {
                console.warn(`Stream request failed: ${response.status}`);
                await sleep(100);
                consecutiveErrors++;
                if (consecutiveErrors >= MAX_CONSECUTIVE_ERRORS) {
                    console.error('Too many consecutive errors, stopping stream');
                    break;
                }
                continue;
            }

            consecutiveErrors = 0;

            // Parse headers
            const sequence = parseInt(response.headers.get('X-Sequence') || '0');
            lastSequence = sequence;

            // Parse binary frame data
            const arrayBuffer = await response.arrayBuffer();
            const frameData = parseStreamFrame(new Uint8Array(arrayBuffer));

            // Decode frame
            await decoder.decodeFrame(frameData);

        } catch (e) {
            console.error('Stream error:', e);
            consecutiveErrors++;
            if (consecutiveErrors >= MAX_CONSECUTIVE_ERRORS) {
                console.error('Too many consecutive errors, stopping stream');
                break;
            }
            await sleep(1000);
        }
    }
}

/**
 * Sleep helper
 */
function sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}
