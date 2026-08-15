import { createApp } from 'vue';
import dayjs from 'dayjs';
import customParseFormat from 'dayjs/plugin/customParseFormat';
import '@fontsource/ibm-plex-sans/400.css';
import '@fontsource/ibm-plex-sans/500.css';
import '@fontsource/ibm-plex-sans/600.css';
import '@fontsource/ibm-plex-sans/700.css';
import '@fontsource/ibm-plex-mono/400.css';
import '@fontsource/ibm-plex-mono/500.css';
import './ui/tokens.css';
import App from './App.vue';
import { i18n } from './i18n';
import { router } from './router';

// `dayjs(date, format)` silently ignores the format without this plugin —
// snapshot ids (YYYY-MM-DD_HH-mm-ss) used to fall back to raw strings.
dayjs.extend(customParseFormat);

createApp(App).use(i18n).use(router).mount('#app');
