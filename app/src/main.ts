import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { applyTheme } from './lib/theme';

applyTheme();

const target = document.getElementById('app');
if (!target) throw new Error('#app missing from index.html');
export default mount(App, { target });
