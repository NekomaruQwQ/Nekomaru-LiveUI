// Ambient types for the AudioWorklet execution context.
//
// TypeScript has no built-in "audioworklet" lib — the WebWorker lib conflicts
// with the DOM lib used by the rest of the frontend.  These declarations give
// worklet.ts just enough type coverage to compile under the main tsconfig.

declare class AudioWorkletProcessor {
    readonly port: MessagePort;
    process(inputs: Float32Array[][], outputs: Float32Array[][], parameters: Record<string, Float32Array>): boolean;
}

declare function registerProcessor(name: string, ctor: new () => AudioWorkletProcessor): void;
