import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { applyTheme } from './lib/theme';

// Resolve data-theme etc. before the first paint (spec 9.2).
applyTheme();

const target = document.getElementById('app');
if (!target) throw new Error('#app missing from index.html');
export default mount(App, { target });
