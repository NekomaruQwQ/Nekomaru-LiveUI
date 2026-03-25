// Static icon lookup — replaces `lucide-react/dynamic` which ships broken ESM
// imports (`.ts` extensions in `.js` files) that Vite 8 / Rolldown rejects.
// As a bonus, this renders synchronously instead of lazy-loading via import().
//
// When adding a new icon: import the PascalCase component from "lucide-react"
// and add a kebab-case entry to ICONS below.

import type { LucideProps } from "lucide-react";
import {
    Activity,
    Clock1, Clock2, Clock3, Clock4, Clock5, Clock6,
    Clock7, Clock8, Clock9, Clock10, Clock11, Clock12,
    Code,
    Coffee,
    Gamepad,
    MessageCircle,
    Mic, MicOff,
    Monitor,
    Music,
} from "lucide-react";

const ICONS = {
    "activity": Activity,
    "clock-1": Clock1, "clock-2": Clock2, "clock-3": Clock3,
    "clock-4": Clock4, "clock-5": Clock5, "clock-6": Clock6,
    "clock-7": Clock7, "clock-8": Clock8, "clock-9": Clock9,
    "clock-10": Clock10, "clock-11": Clock11, "clock-12": Clock12,
    "code": Code,
    "coffee": Coffee,
    "gamepad": Gamepad,
    "message-circle": MessageCircle,
    "mic": Mic, "mic-off": MicOff,
    "monitor": Monitor,
    "music": Music,
} as const satisfies Record<string, React.ComponentType<LucideProps>>;

export type IconName = keyof typeof ICONS;

type IconProps = Omit<LucideProps, "ref"> & { name: IconName };

/// Renders a lucide icon by kebab-case name from a fixed set of static imports.
export default function Icon({ name, ...props }: IconProps) {
    const Component = ICONS[name];
    return <Component {...props} />;
}
