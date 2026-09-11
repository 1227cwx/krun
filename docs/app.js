'use strict';
(() => {
  const $ = (selector) => document.querySelector(selector);
  const items = [
    {name: '文件资源管理器', category: 'daily', icon: '▱', color: '#eee4b9'},
    {name: '记事本', category: 'daily', icon: '▤', color: '#d7e7e2'},
    {name: '计算器', category: 'daily', icon: '⊞', color: '#e0dfda'},
    {name: '终端', category: 'daily', icon: '>_', color: '#d5e0d5'},
    {name: '浏览器', category: 'daily', icon: '◎', color: '#d5e5ec'},
    {name: '任务管理器', category: 'daily', icon: '▥', color: '#dce5f0'},
    {name: '项目文档', category: 'work', icon: '▤', color: '#e4e0ce'},
    {name: '设计素材', category: 'work', icon: '◇', color: '#e7dfd8'},
    {name: '工作文件夹', category: 'work', icon: '▱', color: '#eee4b9'},
    {name: '编辑器', category: 'work', icon: '{ }', color: '#d5e5ec'},
    {name: '命令提示符', category: 'tools', icon: '>_', color: '#d5e0d5'},
    {name: '控制面板', category: 'tools', icon: '⊞', color: '#dce5f0'},
    {name: '字符映射表', category: 'tools', icon: 'Aa', color: '#e6dfeb'}
  ];
  const tabs = [...document.querySelectorAll('[role="tab"]')];
  const grid = $('#demo-grid');
  const search = $('#demo-search');
  const status = $('#demo-status');
  const settings = $('#settings-button');
  let category = 'daily';
  let selected = '';
  const announce = (message) => { status.textContent = message; };
  function select(tile, item) {
    selected = item.name;
    grid.querySelectorAll('button').forEach(button => button.setAttribute('aria-pressed', String(button === tile)));
    announce(`已选择「${item.name}」。双击或按 Enter 模拟运行。`);
  }
  function launch(item) { announce(`已模拟运行「${item.name}」。此演示不会启动本机程序。`); }
  function render() {
    const query = search.value.trim().toLocaleLowerCase();
    const visible = items.filter(item => query ? item.name.toLocaleLowerCase().includes(query) : item.category === category);
    grid.replaceChildren();
    $('#empty').hidden = visible.length > 0;
    visible.forEach((item, index) => {
      const tile = document.createElement('button');
      tile.type = 'button'; tile.className = 'app-tile';
      tile.setAttribute('aria-pressed', String(selected === item.name));
      tile.setAttribute('aria-label', `${item.name}，模拟项目`);
      tile.style.animationDelay = `${index * 25}ms`;
      const icon = document.createElement('span');
      icon.className = 'app-icon'; icon.setAttribute('aria-hidden', 'true');
      icon.textContent = item.icon; icon.style.setProperty('--tile-bg', item.color);
      const label = document.createElement('span'); label.textContent = item.name;
      tile.append(icon, label);
      tile.addEventListener('click', event => {
        select(tile, item);
        if (!$('#double-click').checked || event.detail === 0) launch(item);
      });
      tile.addEventListener('dblclick', () => { if ($('#double-click').checked) launch(item); });
      tile.addEventListener('keydown', event => {
        const buttons = [...grid.children];
        const columns = getComputedStyle(grid).gridTemplateColumns.split(' ').length;
        const offset = {ArrowRight: 1, ArrowLeft: -1, ArrowDown: columns, ArrowUp: -columns}[event.key];
        if (offset !== undefined) {
          event.preventDefault();
          buttons[Math.max(0, Math.min(buttons.length - 1, index + offset))].focus();
        }
      });
      grid.append(tile);
    });
    if (query) announce(`找到 ${visible.length} 个示例项目。搜索范围：所有分类。`);
  }
  function activate(tab) {
    category = tab.dataset.category; selected = ''; search.value = '';
    tabs.forEach(item => { const active = item === tab; item.setAttribute('aria-selected', String(active)); item.tabIndex = active ? 0 : -1; });
    $('#demo-panel').setAttribute('aria-labelledby', tab.id);
    render(); announce(`已切换到「${tab.textContent}」分类。`);
  }
  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => activate(tab));
    tab.addEventListener('keydown', event => {
      let next;
      if (event.key === 'ArrowRight') next = (index + 1) % tabs.length;
      if (event.key === 'ArrowLeft') next = (index + tabs.length - 1) % tabs.length;
      if (event.key === 'Home') next = 0;
      if (event.key === 'End') next = tabs.length - 1;
      if (next !== undefined) { event.preventDefault(); activate(tabs[next]); tabs[next].focus(); }
    });
  });
  search.addEventListener('input', () => { selected = ''; render(); if (!search.value.trim()) announce('已清除搜索，显示当前分类。'); });
  function toggleSettings(open) {
    settings.setAttribute('aria-expanded', String(open));
    settings.textContent = open ? '返回启动板' : '设置';
    $('#launcher-view').hidden = open; $('#demo-settings').hidden = !open;
    announce(open ? '设置仅作用于网页演示。' : '已返回启动板。');
  }
  settings.addEventListener('click', () => toggleSettings(settings.getAttribute('aria-expanded') !== 'true'));
  $('#centered').addEventListener('change', event => {
    $('#movable').disabled = event.target.checked;
    if (event.target.checked) $('#movable').checked = false;
  });
  $('#demo-settings').addEventListener('change', event => {
    const label = event.target.closest('label');
    if (label) announce(`${label.querySelector('span').firstChild.textContent}已${event.target.checked ? '开启' : '关闭'}，仅影响演示。`);
  });
  $('.demo').addEventListener('keydown', event => {
    if (event.key === 'Escape') {
      if (settings.getAttribute('aria-expanded') === 'true') { toggleSettings(false); settings.focus(); }
      else { search.value = ''; selected = ''; render(); search.focus(); announce('已清除搜索与选择。'); }
    }
  });
  render();
  if ('IntersectionObserver' in window && !window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    const observer = new IntersectionObserver(entries => entries.forEach(entry => {
      if (entry.isIntersecting) { entry.target.classList.add('visible'); observer.unobserve(entry.target); }
    }), {threshold: 0.08});
    document.querySelectorAll('.reveal').forEach(section => observer.observe(section));
    document.documentElement.classList.add('motion');
  }
})();
