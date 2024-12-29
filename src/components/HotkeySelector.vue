<script lang="ts" setup>
import { ElSelect, ElOption } from 'element-plus';
import { $t } from "../i18n";

// Initialize hotkey_out with default values
const hotkey_out = defineModel<{
    backup: string[];
    apply: string[];
}>({
    default: {
        backup: ['', '', ''],
        apply: ['', '', ''],
    }
});

// NOTICE: 以下代码来自 https://github.com/tauri-apps/tauri/discussions/7121
// 这些代码是通过该文件获得的： https://github.com/tauri-apps/global-hotkey/blob/c9913a97667b3e44cb000de384cd8937d5a0050a/src/hotkey.rs#L212
// 上述链接目标遵循 Apache-2.0/MIT 协议，本项目根据MIT协议合理使用该代码
// 授权协议文件请参考 https://github.com/tauri-apps/global-hotkey/blob/dev/LICENSE-MIT


// src/schemas/hotkeyMap.ts (7-114)
const otherKeys = [
    'COMMA',
    'EQUAL',
    'MINUS',
    'PERIOD',
    'QUOTE',
    'SEMICOLON',
    'SLASH',
    'BACKSPACE',
    'ENTER',
    'SPACE',
    'TAB',
    'DELETE',
    'END',
    'HOME',
    'INSERT',
    'PAGEDOWN',
    'PAGEUP',
    'ARROWDOWN',
    'ARROWLEFT',
    'ARROWRIGHT',
    'ARROWUP',
    'NUMPADADD',
    'NUMPADDECIMAL',
    'NUMPADDIVIDE',
    'NUMPADENTER',
    'NUMPADEQUAL',
    'NUMPADMULTIPLY',
    'NUMPADSUBTRACT'
];

const funcKeys1 = [
    '',
    'SHIFT',
    'CONTROL',
    'ALT',
];

const funcKeys2 = [
    '',
    '0',
    '1',
    '2',
    '3',
    '4',
    '5',
    '6',
    '7',
    '8',
    '9',
    'A',
    'B',
    'C',
    'D',
    'E',
    'F',
    'G',
    'H',
    'I',
    'J',
    'K',
    'L',
    'M',
    'N',
    'O',
    'P',
    'Q',
    'R',
    'S',
    'T',
    'U',
    'V',
    'W',
    'X',
    'Y',
    'Z',
    // 'NUMPAD0',
    // 'NUMPAD1',
    // 'NUMPAD2',
    // 'NUMPAD3',
    // 'NUMPAD4',
    // 'NUMPAD5',
    // 'NUMPAD6',
    // 'NUMPAD7',
    // 'NUMPAD8',
    // 'NUMPAD9',
    'F1',
    'F2',
    'F3',
    'F4',
    'F5',
    'F6',
    'F7',
    'F8',
    'F9',
    'F10',
    'F11',
    'F12',
    'F13',
    'F14',
    'F15',
    'F16',
    'F17',
    'F18',
    'F19',
    'F20',
    'F21',
    'F22',
    'F23',
    'F24'
]
</script>

<template>
    <div>
        <div>
            <strong>{{ $t("settings.hotkey.hint") }}</strong>
            <ElRow>
                <span>{{ $t("settings.hotkey.quick_backup") }}</span>
            </ElRow>
            <ElRow>
                <ElSelect v-model="hotkey_out.backup[0]" placeholder="Select Key 1">
                    <ElOption v-for="key in funcKeys1" :key="key" :label="key" :value="key" />
                </ElSelect>
                <ElSelect v-model="hotkey_out.backup[1]" placeholder="Select Key 2">
                    <ElOption v-for="key in funcKeys1" :key="key" :label="key" :value="key" />
                </ElSelect>
                <ElSelect v-model="hotkey_out.backup[2]" placeholder="Select Key 3">
                    <ElOption v-for="key in funcKeys2" :key="key" :label="key" :value="key" />
                </ElSelect>
            </ElRow>
        </div>
        <div>
            <ElRow>
                <span>{{ $t("settings.hotkey.quick_apply") }}</span>
            </ElRow>
            <ElRow>
                <ElSelect v-model="hotkey_out.apply[0]" placeholder="Select Key 1">
                    <ElOption v-for="key in funcKeys1" :key="key" :label="key" :value="key" />
                </ElSelect>
                <ElSelect v-model="hotkey_out.apply[1]" placeholder="Select Key 2">
                    <ElOption v-for="key in funcKeys1" :key="key" :label="key" :value="key" />
                </ElSelect>
                <ElSelect v-model="hotkey_out.apply[2]" placeholder="Select Key 3">
                    <ElOption v-for="key in funcKeys2" :key="key" :label="key" :value="key" />
                </ElSelect>
            </ElRow>
        </div>
    </div>
</template>

<style scoped>
.el-select {
    width: 120px;
    margin-left: 8px;
}
</style>
