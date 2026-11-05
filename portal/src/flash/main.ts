import { mount } from 'svelte';
import '@fontsource-variable/inter';
import Flash from './Flash.svelte';
import '../app.css';

export default mount(Flash, { target: document.getElementById('app')! });
