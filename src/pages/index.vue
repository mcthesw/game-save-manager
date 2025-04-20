<template>
    <ElContainer class="home-container" direction="vertical">
        <div class="hero-section">
            <h2 class="welcome-title">{{ $t('home.hello_world') }}</h2>
            <div class="intro-box">
                <div class="intro-content">
                    <h3 class="intro-title">{{ $t("home.name") }}</h3>
                    <p class="intro-text">{{ $t('home.simple_explained') }}</p>
                    <div class="feature-pills">
                        <div class="feature-pill">
                            <el-icon>
                                <Check />
                            </el-icon>
                            <span>{{ $t('home.simple') }}</span>
                        </div>
                        <div class="feature-pill">
                            <el-icon>
                                <Lock />
                            </el-icon>
                            <span>{{ $t('home.safe') }}</span>
                        </div>
                        <div class="feature-pill">
                            <el-icon>
                                <Star />
                            </el-icon>
                            <span>{{ $t('home.free') }}</span>
                        </div>
                    </div>
                </div>
            </div>
        </div>

        <div class="features-grid">
            <div class="feature-card" @click="go_add_game()">
                <el-icon class="feature-icon">
                    <Edit />
                </el-icon>
                <h3>{{ $t('home.import_game') }}</h3>
                <el-button type="primary" text>{{ $t('home.jump_to_page') }}</el-button>
            </div>

            <div class="feature-card" @click="go_settings()">
                <div class="feature-icon language-icon">
                    <span class="language-icon-text">A</span>
                    <span class="language-icon-arrow">/</span>
                    <span class="language-icon-text">文</span>
                </div>
                <div class="language-carousel-container">
                    <transition name="fade" mode="out-in">
                        <div :key="currentLanguageIndex" class="language-name">
                            {{ displayedLanguageName }}
                        </div>
                    </transition>
                </div>
                <el-button type="primary" text>{{ $t('home.jump_to_page') }}</el-button>
            </div>

            <div class="feature-card" @click="go_backup()">
                <el-icon class="feature-icon">
                    <Upload />
                </el-icon>
                <h3>{{ $t('home.start_backup') }}</h3>
                <el-button type="primary" text>{{ $t('home.hint') }}</el-button>
            </div>
        </div>
    </ElContainer>
</template>

<script lang="ts" setup>
import { Edit, Setting, Upload, Check, Lock, Star } from "@element-plus/icons-vue";
import { $t, getSupportedLanguages } from "../i18n";
import { i18n } from "../i18n";
import { ref, onMounted, onBeforeUnmount, computed } from 'vue';
const { showInfo } = useNotification();

// 语言轮播相关代码
const languages = getSupportedLanguages();

const currentLanguageIndex = ref(0);
const intervalId = ref<number | null>(null);

const displayedLanguageName = computed(() => {
    return languages[currentLanguageIndex.value].name;
});

// 启动轮播
const startCarousel = () => {
    intervalId.value = window.setInterval(() => {
        currentLanguageIndex.value = (currentLanguageIndex.value + 1) % languages.length;
    }, 2000); // 每2秒切换一次
};

// 停止轮播
const stopCarousel = () => {
    if (intervalId.value !== null) {
        clearInterval(intervalId.value);
        intervalId.value = null;
    }
};

// 组件挂载时启动轮播
onMounted(() => {
    // 找到当前语言的索引作为起始点
    const currentLocale = i18n.global.locale.value;
    const index = languages.findIndex(lang => lang.code === currentLocale);
    if (index !== -1) {
        currentLanguageIndex.value = index;
    }

    startCarousel();
});

// 组件卸载前停止轮播
onBeforeUnmount(() => {
    stopCarousel();
});

function go_add_game() {
    navigateTo("/AddGame");
}
function go_settings() {
    navigateTo("/Settings")
}
function go_backup() {
    showInfo({
        message: $t('home.go_backup_hint')
    });
}
</script>

<style scoped>
.hero-section {
    padding: 2rem 0;
    text-align: center;
    background: linear-gradient(135deg, var(--el-bg-color), var(--el-bg-color-overlay));
    border-radius: 20px;
    margin: 0 2rem;
    position: relative;
}



.welcome-title {
    font-size: 2.8em;
    margin-bottom: 1.5rem;
    background: linear-gradient(45deg, var(--el-color-primary), var(--el-color-success));
    background-clip: text;
    -webkit-background-clip: text;
    color: transparent;
    text-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
}

.intro-box {
    max-width: 800px;
    margin: 0 auto;
    padding: 2rem;
}

.logo-container {
    margin-bottom: 1.5rem;
}

.intro-icon {
    font-size: 3rem;
    color: var(--el-color-primary);
    padding: 1rem;
    border-radius: 50%;
    background: var(--el-bg-color);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.1);
}

.intro-title {
    font-size: 1.8rem;
    margin-bottom: 1rem;
    color: var(--el-text-color-primary);
}

.intro-text {
    font-size: 1.1rem;
    line-height: 1.6;
    color: var(--el-text-color-regular);
    margin-bottom: 2rem;
}

.feature-pills {
    display: flex;
    justify-content: center;
    gap: 1rem;
    flex-wrap: wrap;
}

.feature-pill {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem;
    background: var(--el-bg-color);
    border-radius: 20px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.05);
    color: var(--el-text-color-primary);
}

.feature-pill .el-icon {
    color: var(--el-color-primary);
}

.features-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 2rem;
    padding: 3rem 2rem;
}

.feature-card {
    background: var(--el-bg-color);
    border-radius: 16px;
    padding: 2rem;
    text-align: center;
    transition: all 0.3s ease;
    cursor: pointer;
    position: relative;
    overflow: hidden;
}

.feature-card::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 4px;
    background: linear-gradient(90deg, var(--el-color-primary), var(--el-color-success));
    opacity: 0;
    transition: opacity 0.3s ease;
}

.feature-card:hover {
    transform: translateY(-5px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.1);
}

.feature-card:hover::before {
    opacity: 1;
}

.feature-icon {
    font-size: 2.5rem;
    color: var(--el-color-primary);
    margin-bottom: 1rem;
}

.language-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.3rem;
}

.language-icon-text {
    font-size: 1.8rem;
    font-weight: bold;
    margin-top: 3px;
    margin-bottom: 17px;
}

.language-icon-arrow {
    color: var(--el-color-primary);
    font-size: 1.5rem;
}

.feature-card h3 {
    font-size: 1.4rem;
    margin-bottom: 0.8rem;
    color: var(--el-text-color-primary);
}

.language-carousel-container {
    margin: 1rem 0;
}

.language-name {
    font-size: 1.2rem;
    font-weight: 500;
    margin-bottom: 0.5rem;
    min-height: 1.8rem;
}

/* 过渡动画 */
.fade-enter-active,
.fade-leave-active {
    transition: opacity 0.5s ease, transform 0.5s ease;
}

.fade-enter-from,
.fade-leave-to {
    opacity: 0;
    transform: translateY(10px);
}

.feature-card p {
    color: var(--el-text-color-secondary);
    margin-bottom: 1.5rem;
}

@media (max-width: 768px) {
    .hero-section {
        margin: 0 1rem;
        padding: 1.5rem 0;
    }

    .welcome-title {
        font-size: 2.2em;
    }

    .features-grid {
        grid-template-columns: 1fr;
        padding: 2rem 1rem;
    }
}
</style>