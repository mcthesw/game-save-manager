<script lang="ts" setup>
import { useConfig } from "../stores/ConfigFile";
const config = useConfig();
function source_click(url: string) {
    //TODO 进入链接
}
const thanks = [
    { name: "Sworld", describe: "本项目的创始者" },
    { name: "勺子", describe: "早期测试者" },
    { name: "Wali", describe: "早期测试者" },
    { name: "土拨鼠", describe: "早期测试者" },
    { name: "布莱泽", describe: "早期测试者" },
]
const frames = [
    { name: "Vue3", describe: "前端框架" },
    { name: "Tauri", describe: "桌面应用框架" },
    { name: "Element-plus", describe: "UI框架" },
]
</script>

<template>
    <el-container>
        <el-main class="about-content">
            <h2>该软件是一个开源的，由Sworld主导的游戏存档管理软件</h2>
            <p>
                设计该软件的初衷是为了方便玩家们管理自己的游戏存档，但是由于游戏数量众多，难以一一适配，
                所以游戏路径只能靠手动导入，如果你感兴趣的话，可以尝试参与改善本项目。
            </p>
            <h2>支持我</h2>
            <p>
                你可以点击下方的“开源地址”，在上面为我点一个Star，这是对我很大的鼓励，也可以通过开源平台为这个项目贡献代码和提出问题。
            </p>
            <p>
                如果你是普通用户，可以考虑请我喝杯饮料，我会非常感激。
            </p>
            <el-container direction="horizontal" class="thanks-container">
                <div class="thanks">
                    <el-scrollbar>
                        <h1>感谢名单</h1>
                        <el-timeline>
                            <el-timeline-item v-for="thank in thanks" :key="thank.name" :timestamp="thank.describe">
                                {{ thank.name }}
                            </el-timeline-item>
                        </el-timeline>
                    </el-scrollbar>
                </div>
                <div class="frames">
                    <el-scrollbar>
                        <h1>使用框架</h1>
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
                source_click('https://gitee.com/sworldS/game-save-manager/releases/')
            ">国内发布地址</el-link>
            <span class="version">版本号: {{ config.version }}</span>
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