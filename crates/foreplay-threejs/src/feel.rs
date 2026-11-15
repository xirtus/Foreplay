//! 3JSE `FeelSpec` store generator and Live Tuning DevHUD codegen.

use foreplay_ir::{FeelSpecKnob, IrScene};
use std::collections::BTreeMap;

/// Collects all feel knobs across scene overrides and script modules, ensuring deterministic ordering.
pub fn collect_feel_knobs(
    scene: &IrScene,
    script_knobs: &[FeelSpecKnob],
) -> Vec<FeelSpecKnob> {
    let mut knob_map: BTreeMap<String, FeelSpecKnob> = BTreeMap::new();

    // 1. Script-declared @export_range knobs
    for knob in script_knobs {
        knob_map.insert(knob.key.clone(), knob.clone());
    }

    // 2. Scene-level feel overrides
    for (key, &val) in &scene.feel_overrides {
        if let Some(existing) = knob_map.get_mut(key) {
            existing.default_value = val;
        } else {
            let (min, max) = if val < 0.0 {
                (val * 2.0, 0.0)
            } else if val == 0.0 {
                (0.0, 10.0)
            } else {
                (0.0, (val * 2.0).max(10.0))
            };
            knob_map.insert(
                key.clone(),
                FeelSpecKnob {
                    key: key.clone(),
                    intent: key.clone(),
                    unit: String::new(),
                    min,
                    max,
                    default_value: val,
                },
            );
        }
    }

    knob_map.into_values().collect()
}

/// Generates `src/feel.ts` TypeScript source code for the 3JSE FeelSpec store.
pub fn generate_feel_ts(knobs: &[FeelSpecKnob], overrides: &BTreeMap<String, f64>) -> String {
    let mut ts = String::new();

    ts.push_str(
        "// Foreplay 3JSE FeelSpec Store & Live Tuning DevHUD\n\
        // Generated automatically by foreplay-threejs compiler.\n\n\
        export interface FeelKnob {\n\
            key: string;\n\
            label: string;\n\
            min: number;\n\
            max: number;\n\
            step: number;\n\
            defaultValue: number;\n\
            unit: string;\n\
        }\n\n",
    );

    // List of knobs
    ts.push_str("export const FEEL_KNOBS: FeelKnob[] = [\n");
    for k in knobs {
        let step = compute_sensible_step(k.min, k.max);
        let default_val = overrides.get(&k.key).copied().unwrap_or(k.default_value);
        ts.push_str(&format!(
            "    {{\n\
            \x20       key: {:?},\n\
            \x20       label: {:?},\n\
            \x20       min: {},\n\
            \x20       max: {},\n\
            \x20       step: {},\n\
            \x20       defaultValue: {},\n\
            \x20       unit: {:?},\n\
            \x20   }},\n",
            k.key,
            if k.intent.is_empty() { &k.key } else { &k.intent },
            format_f64(k.min),
            format_f64(k.max),
            format_f64(step),
            format_f64(default_val),
            k.unit
        ));
    }
    ts.push_str("];\n\n");

    // Default values dictionary
    ts.push_str("export const DEFAULT_FEEL: Record<string, number> = {\n");
    for k in knobs {
        let val = overrides.get(&k.key).copied().unwrap_or(k.default_value);
        ts.push_str(&format!("    {:?}: {},\n", k.key, format_f64(val)));
    }
    ts.push_str("};\n\n");

    // FeelStore implementation
    ts.push_str(
        "export type FeelListener = (value: number) => void;\n\
        export type GlobalFeelListener = (key: string, value: number) => void;\n\n\
        export class FeelStore {\n\
            private values: Record<string, number>;\n\
            private listeners: Map<string, FeelListener[]> = new Map();\n\
            private globalListeners: GlobalFeelListener[] = [];\n\n\
            constructor(initial: Record<string, number> = DEFAULT_FEEL) {\n\
                this.values = { ...initial };\n\
            }\n\n\
            get(key: string, fallback: number = 0): number {\n\
                return this.values[key] ?? fallback;\n\
            }\n\n\
            set(key: string, value: number): void {\n\
                this.values[key] = value;\n\
                const subs = this.listeners.get(key);\n\
                if (subs) {\n\
                    for (const cb of subs) cb(value);\n\
                }\n\
                for (const gcb of this.globalListeners) gcb(key, value);\n\
            }\n\n\
            subscribe(key: string, callback: FeelListener): () => void {\n\
                if (!this.listeners.has(key)) {\n\
                    this.listeners.set(key, []);\n\
                }\n\
                this.listeners.get(key)!.push(callback);\n\
                callback(this.get(key));\n\
                return () => {\n\
                    const arr = this.listeners.get(key);\n\
                    if (arr) {\n\
                        const idx = arr.indexOf(callback);\n\
                        if (idx !== -1) arr.splice(idx, 1);\n\
                    }\n\
                };\n\
            }\n\n\
            subscribeAll(callback: GlobalFeelListener): () => void {\n\
                this.globalListeners.push(callback);\n\
                return () => {\n\
                    const idx = this.globalListeners.indexOf(callback);\n\
                    if (idx !== -1) this.globalListeners.splice(idx, 1);\n\
                };\n\
            }\n\n\
            reset(): void {\n\
                for (const knob of FEEL_KNOBS) {\n\
                    this.set(knob.key, knob.defaultValue);\n\
                }\n\
            }\n\n\
            getAll(): Record<string, number> {\n\
                return { ...this.values };\n\
            }\n\
        }\n\n\
        export const feelStore = new FeelStore(DEFAULT_FEEL);\n\
        if (typeof window !== 'undefined') {\n\
            (window as any).__FOREPLAY_FEEL__ = feelStore;\n\
        }\n\n",
    );

    // DevHUD mounting helper
    ts.push_str(
        "export function mountFeelDevHud(store: FeelStore = feelStore, knobs: FeelKnob[] = FEEL_KNOBS): HTMLElement {\n\
            const existing = document.getElementById('feel-dev-hud');\n\
            if (existing) existing.remove();\n\n\
            const hud = document.createElement('div');\n\
            hud.id = 'feel-dev-hud';\n\
            hud.className = 'feel-hud-container';\n\n\
            let collapsed = false;\n\
            const header = document.createElement('div');\n\
            header.className = 'feel-hud-header';\n\
            header.innerHTML = `\n\
                <span class=\"feel-hud-title\">⚡ 3JSE FeelSpec HUD</span>\n\
                <div class=\"feel-hud-actions\">\n\
                    <button id=\"feel-hud-reset\" title=\"Reset to defaults\">↺</button>\n\
                    <button id=\"feel-hud-toggle\" title=\"Collapse HUD\">−</button>\n\
                </div>\n\
            `;\n\
            hud.appendChild(header);\n\n\
            const body = document.createElement('div');\n\
            body.className = 'feel-hud-body';\n\n\
            for (const knob of knobs) {\n\
                const row = document.createElement('div');\n\
                row.className = 'feel-hud-row';\n\
                const currentVal = store.get(knob.key, knob.defaultValue);\n\
                row.innerHTML = `\n\
                    <div class=\"feel-hud-row-meta\">\n\
                        <span class=\"feel-hud-label\">${knob.label}</span>\n\
                        <span class=\"feel-hud-val\" id=\"val-${knob.key}\">${currentVal.toFixed(2)}${knob.unit ? ' ' + knob.unit : ''}</span>\n\
                    </div>\n\
                    <input type=\"range\" class=\"feel-hud-slider\" id=\"slider-${knob.key}\"\n\
                        min=\"${knob.min}\" max=\"${knob.max}\" step=\"${knob.step}\" value=\"${currentVal}\" />\n\
                `;\n\
                const slider = row.querySelector(`#slider-${knob.key}`) as HTMLInputElement;\n\
                const readout = row.querySelector(`#val-${knob.key}`) as HTMLElement;\n\
                slider.addEventListener('input', () => {\n\
                    const v = parseFloat(slider.value);\n\
                    store.set(knob.key, v);\n\
                    readout.textContent = `${v.toFixed(2)}${knob.unit ? ' ' + knob.unit : ''}`;\n\
                });\n\
                store.subscribe(knob.key, (v) => {\n\
                    slider.value = v.toString();\n\
                    readout.textContent = `${v.toFixed(2)}${knob.unit ? ' ' + knob.unit : ''}`;\n\
                });\n\
                body.appendChild(row);\n\
            }\n\
            hud.appendChild(body);\n\n\
            const toggleBtn = header.querySelector('#feel-hud-toggle') as HTMLButtonElement;\n\
            toggleBtn.addEventListener('click', () => {\n\
                collapsed = !collapsed;\n\
                body.style.display = collapsed ? 'none' : 'block';\n\
                toggleBtn.textContent = collapsed ? '+' : '−';\n\
            });\n\n\
            const resetBtn = header.querySelector('#feel-hud-reset') as HTMLButtonElement;\n\
            resetBtn.addEventListener('click', () => {\n\
                store.reset();\n\
            });\n\n\
            document.body.appendChild(hud);\n\
            return hud;\n\
        }\n",
    );

    ts
}

fn compute_sensible_step(min: f64, max: f64) -> f64 {
    let span = (max - min).abs();
    if span <= 1.0 {
        0.01
    } else if span <= 10.0 {
        0.05
    } else if span <= 50.0 {
        0.1
    } else {
        0.5
    }
}

fn format_f64(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feel_knobs_collection_and_ts_generation() {
        let mut scene = IrScene::new("FeelTuningTest");
        scene.feel_overrides.insert("gravity".into(), 9.81);
        scene.feel_overrides.insert("skate_speed".into(), 16.0);

        let script_knob = FeelSpecKnob {
            key: "ollie_pop".into(),
            intent: "Ollie pop impulse".into(),
            unit: "N".into(),
            min: 0.0,
            max: 20.0,
            default_value: 8.5,
        };

        let knobs = collect_feel_knobs(&scene, &[script_knob]);
        assert_eq!(knobs.len(), 3);

        let ts = generate_feel_ts(&knobs, &scene.feel_overrides);
        assert!(ts.contains("export interface FeelKnob"));
        assert!(ts.contains("FEEL_KNOBS: FeelKnob[]"));
        assert!(ts.contains("__FOREPLAY_FEEL__ = feelStore"));
        assert!(ts.contains("ollie_pop"));
        assert!(ts.contains("skate_speed"));
        assert!(ts.contains("gravity"));
        assert!(ts.contains("mountFeelDevHud"));
    }
}
