<script lang="ts" setup>
import { commands } from "~/bindings";
import { $t } from "../i18n";
import { app } from "@tauri-apps/api";
import { debug } from "@tauri-apps/plugin-log";

const { showError } = useNotification();
const { config } = useConfig();
async function source_click(url: string) {
    debug(`open url ${url}`)
    try { await commands.openUrl(url) } catch (e) { showError({ message: $t('error.open_url_failed') }) }
};

const thanks = [
    { name: "Sworld", describe: $t('about.project_initiator') },
    { name: "Itsusinn逸新", describe: $t('about.developer') },
    { name: "AsterNighT", describe: $t('about.developer') },
    { name: "noneSycamore", describe: $t('about.developer') },
    { name: "勺子", describe: $t('about.ea_tester') },
    { name: "Wali", describe: $t('about.ea_tester') },
    { name: "土拨鼠", describe: $t('about.ea_tester') },
    { name: "布莱泽", describe: $t('about.ea_tester') },
]
const frames = [
    { name: "Vue3", describe: $t('about.frontend_framework') },
    { name: "Tauri", describe: $t('about.desktop_framework') },
    { name: "Element-plus", describe: $t('about.ui_framework') },
]
</script>

<template>
    <el-container>
        <el-main class="about-content">
            <h2>{{ $t('about.content_1') }}</h2>
            <p>
                {{ $t('about.content_2') }} </p>
            <h2>{{ $t('about.support_me') }}</h2>
            <p>
                {{ $t('about.support_me_content_1') }} </p>
            <p>
                {{ $t('about.support_me_content_2') }} </p>
            <el-container direction="horizontal" class="thanks-container">
                <div class="thanks">
                    <el-scrollbar>
                        <h1>{{ $t('about.thank_you_list') }}</h1>
                        <el-timeline>
                            <el-timeline-item v-for="thank in thanks" :key="thank.name" :timestamp="thank.describe">
                                {{ thank.name }}
                            </el-timeline-item>
                        </el-timeline>
                    </el-scrollbar>
                </div>
                <div class="frames">
                    <el-scrollbar>
                        <h1>{{ $t('about.frameworks') }}</h1>
                        <el-timeline>
                            <el-timeline-item v-for="frame in frames" :key="frame.name" :timestamp="frame.describe">
                                {{ frame.name }}
                            </el-timeline-item>
                        </el-timeline>
                    </el-scrollbar>
                </div>
            </el-container>
        </el-main>
        <el-footer>
            <el-link @click="source_click('https://gitee.com/sworldS/game-save-manager')">Gitee</el-link>
            |
            <el-link @click="source_click('https://github.com/mcthesw/game-save-manager')">Github</el-link>
            |
            <el-link @click="
                source_click('https://game.sworld.club/')">
                {{ $t('about.official_website') }}
            </el-link>
            |
            <el-link @click="source_click('https://help.sworld.club/')">{{ $t('about.help') }}</el-link>
            <span class="version">{{ $t('about.version') + config?.version }}</span>
        </el-footer>
    </el-container>
</template>

<style scoped>
.version {
    font-size: 0.8rem;
    text-align: right;
    float: right;
}

.thanks-container {
    justify-content: space-around;
    margin-top: 20px;
    height: 250px;
}

.frames,
.thanks {
    width: 45%;
    height: 250px;
}

.thanks h1,
.frames h1 {
    margin-top: 0;
}
</style>