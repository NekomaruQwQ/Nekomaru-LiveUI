export interface NALUnitData {
    type: number;
    data: Uint8Array;
}

export interface StreamFrameData {
    timestamp: number;
    nalUnits: NALUnitData[];
    isKeyframe: boolean;
}

/**
 * H.264 decoder using WebCodecs API
 */
export class H264Decoder {
    private decoder: VideoDecoder | null = null;
    private onFrame: (frame: VideoFrame) => void;
    private isConfigured = false;

    constructor(onFrame: (frame: VideoFrame) => void) {
        this.onFrame = onFrame;
    }

    /**
     * Initialize the decoder by fetching codec parameters from the stream
     */
    async init() {
        // Fetch SPS/PPS from stream://init
        const response = await fetch('stream://init');
        if (!response.ok) {
            throw new Error(`Failed to fetch codec params: ${response.statusText}`);
        }

        const params = await response.json();
        const sps = base64ToUint8Array(params.sps);
        const pps = base64ToUint8Array(params.pps);
        const width = params.width;
        const height = params.height;

        // Parse profile/level from SPS (bytes 1-3)
        const profile = sps[1];
        const level = sps[3];

        // Build codec string (e.g., "avc1.42001f" for baseline profile level 3.1)
        const codecString = `avc1.${toHex(profile)}00${toHex(level)}`;

        // Build avcC descriptor (ISO 14496-15 format)
        const avcC = buildAvcCDescriptor(sps, pps);

        this.decoder = new VideoDecoder({
            output: (frame) => this.handleFrame(frame),
            error: (e) => console.error('Decoder error:', e),
        });

        const config: VideoDecoderConfig = {
            codec: codecString,
            codedWidth: width,
            codedHeight: height,
            description: avcC,
        };

        this.decoder.configure(config);
        this.isConfigured = true;

        console.log(`Decoder initialized: ${codecString}, ${width}x${height}`);
    }

    /**
     * Decode a frame
     */
    async decodeFrame(frameData: StreamFrameData) {
        if (!this.decoder || !this.isConfigured) {
            throw new Error('Decoder not initialized');
        }

        // Concatenate all NAL units for this frame
        const totalSize = frameData.nalUnits.reduce((sum, unit) => sum + unit.data.length, 0);
        const combined = new Uint8Array(totalSize);

        let offset = 0;
        for (const unit of frameData.nalUnits) {
            combined.set(unit.data, offset);
            offset += unit.data.length;
        }

        const chunk = new EncodedVideoChunk({
            type: frameData.isKeyframe ? 'key' : 'delta',
            timestamp: frameData.timestamp,
            data: combined,
        });

        this.decoder.decode(chunk);
    }

    private handleFrame(frame: VideoFrame) {
        this.onFrame(frame);
    }

    close() {
        if (this.decoder) {
            this.decoder.close();
            this.decoder = null;
        }
    }
}

/**
 * Build avcC descriptor for H.264 decoder configuration (ISO 14496-15 format)
 */
function buildAvcCDescriptor(sps: Uint8Array, pps: Uint8Array): Uint8Array {
    const spsLength = sps.length;
    const ppsLength = pps.length;

    const avcC = new Uint8Array(
        1 +  // configurationVersion
        3 +  // AVCProfileIndication, profile_compatibility, AVCLevelIndication
        1 +  // lengthSizeMinusOne
        1 +  // numOfSequenceParameterSets
        2 + spsLength +  // SPS length (16-bit) + data
        1 +  // numOfPictureParameterSets
        2 + ppsLength    // PPS length (16-bit) + data
    );

    let offset = 0;

    // configurationVersion = 1
    avcC[offset++] = 1;

    // Copy profile/level from SPS (bytes 1-3)
    avcC[offset++] = sps[1];  // AVCProfileIndication
    avcC[offset++] = sps[2];  // profile_compatibility
    avcC[offset++] = sps[3];  // AVCLevelIndication

    // lengthSizeMinusOne = 0xFF (4 bytes)
    avcC[offset++] = 0xFF;

    // numOfSequenceParameterSets = 1
    avcC[offset++] = 0xE1;

    // SPS length (16-bit big-endian)
    avcC[offset++] = (spsLength >> 8) & 0xFF;
    avcC[offset++] = spsLength & 0xFF;

    // SPS data
    avcC.set(sps, offset);
    offset += spsLength;

    // numOfPictureParameterSets = 1
    avcC[offset++] = 1;

    // PPS length (16-bit big-endian)
    avcC[offset++] = (ppsLength >> 8) & 0xFF;
    avcC[offset++] = ppsLength & 0xFF;

    // PPS data
    avcC.set(pps, offset);

    return avcC;
}

/**
 * Convert base64 string to Uint8Array
 */
function base64ToUint8Array(base64: string): Uint8Array {
    const binaryString = atob(base64);
    const len = binaryString.length;
    const bytes = new Uint8Array(len);
    for (let i = 0; i < len; i++) {
        bytes[i] = binaryString.charCodeAt(i);
    }
    return bytes;
}

/**
 * Convert number to 2-digit hex string
 */
function toHex(value: number): string {
    return value.toString(16).padStart(2, '0');
}

/**
 * Parse binary stream frame data
 */
export function parseStreamFrame(buffer: Uint8Array): StreamFrameData {
    const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);

    let offset = 0;

    // Read timestamp (u64 little-endian)
    const timestamp = Number(view.getBigUint64(offset, true));
    offset += 8;

    // Read number of NAL units (u32 little-endian)
    const numNalUnits = view.getUint32(offset, true);
    offset += 4;

    const nalUnits: NALUnitData[] = [];
    let isKeyframe = false;

    for (let i = 0; i < numNalUnits; i++) {
        // Read NAL unit type (u8)
        const type = view.getUint8(offset);
        offset += 1;

        // Read data length (u32 little-endian)
        const dataLength = view.getUint32(offset, true);
        offset += 4;

        // Read data
        const data = buffer.slice(offset, offset + dataLength);
        offset += dataLength;

        nalUnits.push({ type, data });

        // Check if this is an IDR frame (type 5)
        if (type === 5) {
            isKeyframe = true;
        }
    }

    return { timestamp, nalUnits, isKeyframe };
}
