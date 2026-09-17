'use strict';
(() => {
  const $ = selector => document.querySelector(selector);
  // Shared eight-tooth polar geometry with native render::gear_points.
  const gearPoints = Array.from({length: 8}, (_, tooth) =>
    [[-22.5, 7], [-13, 7], [-10, 9], [10, 9], [13, 7]].map(([offset, radius]) => {
      const angle = (tooth * 45 + offset - 90) * Math.PI / 180;
      return `${12 + radius * Math.cos(angle)},${12 + radius * Math.sin(angle)}`;
    })).flat().join(' ');
  const paths = {
    search: '<circle cx="11" cy="11" r="7"/><path d="m17 17 6 6"/>',
    plus: '<path d="M5 12h14M12 5v14"/>',
    close: '<path d="m5 5 14 14M19 5 5 19"/>',
    back: '<path d="m15 5-7 7 7 7"/>',
    right: '<path d="m9 5 7 7-7 7"/>',
    more: '<g fill="currentColor" stroke="none"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></g>',
    settings: `<polygon points="${gearPoints}" stroke-linejoin="round"/><circle cx="12" cy="12" r="3"/>`
  };
  document.querySelectorAll('[data-icon]').forEach(button => {
    button.type = 'button';
    button.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${paths[button.dataset.icon]}</svg>`;
  });
  // Icons come from the sanitized native screenshot and Windows Paint's app assets.
  const categories = ['常用', '工作', '工具', '文件'];
  const items = [
    ['文件资源管理器', 'explorer', 0, 'C:/Windows/explorer.exe'],
    ['记事本', 'notepad', 0, 'C:/Windows/notepad.exe'],
    ['计算器', 'calculator', 0, 'C:/Windows/System32/calc.exe'],
    ['命令提示符', 'terminal', 0, 'C:/Windows/System32/cmd.exe'],
    ['画图', 'paint', 0, 'mspaint.exe'],
    ['控制面板', 'control-panel', 0, 'C:/Windows/System32/control.exe'],
    ['字符映射表', 'character-map', 0, 'C:/Windows/System32/charmap.exe'],
    ['任务管理器', 'task-manager', 0, 'C:/Windows/System32/Taskmgr.exe'],
    ['项目文档', 'notepad', 1, '示例/项目文档.txt'],
    ['工作文件夹', 'explorer', 1, '示例/工作'],
    ['命令提示符', 'terminal', 2, 'C:/Windows/System32/cmd.exe'],
    ['控制面板', 'control-panel', 2, 'C:/Windows/System32/control.exe'],
    ['文档', 'explorer', 3, '示例/文档'],
    ['图片', 'explorer', 3, '示例/图片']
  ].map(([name, icon, category, path], id) => ({name, icon, category, path, id}));
  let category = 0;
  let categoryStart = 0;
  let selected = null;
  let settingsOpen = false;
  let capturing = false;
  let hotkey = 'Alt + Q';
  let dialog = null;
  let returnFocus = null;
  let closed = false;
  let contextItem = null;
  let contextCategory = null;
  let contextTrigger = null;
  let itemRenameMode = false;
  let nextItemId = items.length;
  const windowElement = $('.app-window');
  const grid = $('#demo-grid');
  const search = $('#demo-search');

  function renderItems() {
    const query = search.value.trim().toLocaleLowerCase();
    const visible = items.filter(item => query
      ? `${item.name} ${item.path}`.toLocaleLowerCase().includes(query)
      : item.category === category);
    grid.replaceChildren();
    $('#empty').hidden = visible.length > 0;
    $('#empty').textContent = query ? '没有匹配的项目' : '拖入文件，或点击右上角添加';
    visible.forEach((item, index) => {
      const tile = document.createElement('button');
      tile.type = 'button';
      tile.className = 'app-tile';
      tile.setAttribute('aria-pressed', String(item.id === selected));
      tile.title = item.name;
      const image = document.createElement('img');
      image.src = item.customIcon || `images/apps/${item.icon}.png`;
      image.alt = '';
      image.draggable = false;
      const label = document.createElement('span');
      label.textContent = item.name;
      tile.append(image, label);
      tile.addEventListener('click', () => {
        selected = item.id;
        [...grid.children].forEach(button => {
          button.setAttribute('aria-pressed', String(button === tile));
          button.classList.remove('keyboard-selected');
        });
        // Native mouse selection has no persistent background after pointer exit.
      });
      tile.addEventListener('contextmenu', event => {
        event.preventDefault();
        contextItem = item;
        contextTrigger = tile;
        selected = item.id;
        setAddMenu(false);
        setMenu(false);
        hideContextMenus();
        placeContextMenu($('#item-menu'), event);
        $('#item-restore-icon').disabled = !item.customIcon;
        [...grid.children].forEach(button => button.setAttribute('aria-pressed', String(button === tile)));
      });
      tile.addEventListener('keydown', event => {
        if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
          event.preventDefault();
          const rect = tile.getBoundingClientRect();
          tile.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2}));
          return;
        }
        const columns = getComputedStyle(grid).gridTemplateColumns.split(' ').length;
        const offset = {ArrowLeft: -1, ArrowRight: 1, ArrowUp: -columns, ArrowDown: columns}[event.key];
        if (offset !== undefined) {
          event.preventDefault();
          const targetIndex = Math.max(0, Math.min(visible.length - 1, index + offset));
          selected = visible[targetIndex].id;
          [...grid.children].forEach((button, i) => {
            button.classList.toggle('keyboard-selected', i === targetIndex);
            button.setAttribute('aria-pressed', String(i === targetIndex));
          });
          grid.children[targetIndex].focus();
        }
      });
      grid.append(tile);
    });
  }

  function hideContextMenus(restoreFocus = false) {
    const hadOpenMenu = !$('#item-menu').hidden || !$('#category-context-menu').hidden || !$('#move-menu').hidden;
    $('#item-menu').hidden = true;
    $('#category-context-menu').hidden = true;
    $('#move-menu').hidden = true;
    $('#item-move').setAttribute('aria-expanded', 'false');
    if (restoreFocus && hadOpenMenu) contextTrigger?.focus();
  }
  function placeContextMenu(menu, event) {
    menu.hidden = false;
    const windowRect = windowElement.getBoundingClientRect();
    const width = menu.offsetWidth;
    const height = menu.offsetHeight;
    menu.style.left = `${Math.max(4, Math.min(event.clientX - windowRect.left, windowRect.width - width - 4))}px`;
    menu.style.top = `${Math.max(4, Math.min(event.clientY - windowRect.top, windowRect.height - height - 4))}px`;
    menu.querySelector('button:not(:disabled)')?.focus();
  }
  function setMenu(open) {
    if (open) setAddMenu(false);
    $('#category-menu').hidden = !open;
    $('#category-more').setAttribute('aria-expanded', String(open));
  }
  function activate(index, animate = true, rebuildTabs = false) {
    category = index;
    selected = null;
    search.value = '';
    $('#search-bar').hidden = true;
    $('#search-button').setAttribute('aria-expanded', 'false');
    $('#launcher-view').classList.remove('searching');
    setMenu(false);
    setAddMenu(false);
    hideContextMenus();
    if (rebuildTabs) renderTabs();
    else updateTabs();
    grid.classList.toggle('instant-switch', !animate);
    renderItems();
    if (!animate) requestAnimationFrame(() => grid.classList.remove('instant-switch'));
  }
  function updateTabs() {
    [...$('#tabs').children].forEach(tab => {
      const active = Number(tab.dataset.category) === category;
      tab.setAttribute('aria-selected', String(active));
      tab.tabIndex = active ? 0 : -1;
    });
    $('#demo-panel').setAttribute('aria-label', categories[category]);
    [...$('#category-menu').children].forEach(button => {
      button.setAttribute('aria-checked', String(Number(button.dataset.category) === category));
    });
  }
  function renderTabs() {
    const width = windowElement.clientWidth;
    const tabWidth = name => Math.max(72, Math.min(156, Math.min([...name].length, 10) * 16 + 30));
    const overflow = categories.reduce((sum, name) => sum + tabWidth(name) + 2, 0) - 2 > width - 22;
    ['left', 'right', 'more'].forEach(name => { $(`#category-${name}`).hidden = !overflow; });
    if (!overflow) categoryStart = 0;
    const available = width - (overflow ? 132 : 22);
    let occupied = 0;
    let exhausted = false;
    const tabs = $('#tabs');
    tabs.replaceChildren();
    categories.forEach((name, index) => {
      if (index < categoryStart || exhausted) return;
      if (occupied + tabWidth(name) > available) { exhausted = true; return; }
      occupied += tabWidth(name) + 2;
      const tab = document.createElement('button');
      tab.type = 'button';
      tab.id = `tab-${index}`;
      tab.dataset.category = index;
      tab.textContent = [...name].length > 10 ? [...name].slice(0, 10).join('') + '…' : name;
      tab.style.width = `${tabWidth(name)}px`;
      tab.style.flexShrink = '0';
      tab.setAttribute('role', 'tab');
      tab.setAttribute('aria-selected', String(index === category));
      tab.setAttribute('aria-controls', 'demo-panel');
      tab.tabIndex = index === category ? 0 : -1;
      tab.addEventListener('click', () => activate(index));
      tab.addEventListener('pointerenter', event => {
        if (event.pointerType === 'mouse' && category !== index) activate(index, false);
      });
      tab.addEventListener('contextmenu', event => {
        event.preventDefault();
        contextCategory = index;
        contextTrigger = tab;
        hideContextMenus();
        placeContextMenu($('#category-context-menu'), event);
        $('#category-delete').disabled = categories.length <= 1;
      });
      tab.addEventListener('keydown', event => {
        if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
          event.preventDefault();
          const rect = tab.getBoundingClientRect();
          tab.dispatchEvent(new MouseEvent('contextmenu', {bubbles: true, clientX: rect.left + rect.width / 2, clientY: rect.bottom}));
          return;
        }
        const next = {ArrowRight: (index + 1) % categories.length, ArrowLeft: (index + categories.length - 1) % categories.length, Home: 0, End: categories.length - 1}[event.key];
        if (next !== undefined) {
          event.preventDefault();
          categoryStart = next;
          activate(next, true, true);
          $(`#tab-${next}`).focus();
        }
      });
      tabs.append(tab);
    });
    if (!tabs.querySelector('[tabindex="0"]') && tabs.firstChild) tabs.firstChild.tabIndex = 0;
    $('#demo-panel').setAttribute('aria-label', categories[category]);
    $('#category-left').disabled = categoryStart === 0;
    $('#category-right').disabled = categoryStart + tabs.children.length >= categories.length;
    $('#category-menu').replaceChildren();
    categories.forEach((name, index) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.dataset.category = index;
      button.textContent = name;
      button.setAttribute('role', 'menuitemradio');
      button.setAttribute('aria-checked', String(index === category));
      button.addEventListener('click', () => {
        categoryStart = index;
        activate(index, true, true);
        $(`#tab-${index}`).focus();
      });
      $('#category-menu').append(button);
    });
    updateTabs();
  }
  $('#category-more').addEventListener('click', () => {
    const open = $('#category-menu').hidden;
    setMenu(open);
    if (open) $('#category-menu').querySelector('button').focus();
  });
  $('#category-menu').addEventListener('keydown', event => {
    const buttons = [...$('#category-menu').children];
    const index = buttons.indexOf(document.activeElement);
    const next = {ArrowDown: (index + 1) % buttons.length, ArrowUp: (index + buttons.length - 1) % buttons.length, Home: 0, End: buttons.length - 1}[event.key];
    if (next !== undefined) { event.preventDefault(); buttons[next].focus(); }
  });
  $('#category-left').addEventListener('click', () => { categoryStart = Math.max(0, categoryStart - 1); renderTabs(); });
  $('#category-right').addEventListener('click', () => { categoryStart = Math.min(categories.length - 1, categoryStart + 1); renderTabs(); });
  document.addEventListener('pointerdown', event => {
    if (!event.target.closest('#category-menu, #category-more')) setMenu(false);
    if (!event.target.closest('#add-menu, #add-button')) setAddMenu(false);
    if (!event.target.closest('#item-menu, #category-context-menu')) hideContextMenus();
  });

  function setSearch(open) {
    setAddMenu(false);
    $('#search-bar').hidden = !open;
    $('#search-button').setAttribute('aria-expanded', String(open));
    $('#launcher-view').classList.toggle('searching', open);
    if (open) search.focus();
    else { search.value = ''; selected = null; renderItems(); }
  }
  $('#search-button').addEventListener('click', () => setSearch($('#search-bar').hidden));
  $('#search-close').addEventListener('click', () => { setSearch(false); $('#search-button').focus(); });
  search.addEventListener('input', () => { selected = null; renderItems(); });
  function cancelCapture() {
    capturing = false;
    $('#hotkey').textContent = hotkey;
    $('#hotkey').classList.remove('capturing');
  }
  function setSettings(open) {
    settingsOpen = open;
    cancelCapture();
    setMenu(false);
    setAddMenu(false);
    $('#settings-view').hidden = !open;
    $('#launcher-view').hidden = open;
    $('#back-button').hidden = !open;
    ['search', 'add', 'settings'].forEach(name => { $(`#${name}-button`).hidden = open; });
    $('#settings-button').setAttribute('aria-expanded', String(open));
    $('#window-title').textContent = open ? '设置' : 'KRun';
    (open ? $('#back-button') : $('#settings-button')).focus();
  }
  $('#settings-button').addEventListener('click', () => setSettings(true));
  $('#back-button').addEventListener('click', () => setSettings(false));
  $('#hotkey').addEventListener('click', () => {
    capturing = true;
    $('#hotkey').textContent = '请按组合键...';
    $('#hotkey').classList.add('capturing');
  });
  $('#hotkey').addEventListener('blur', cancelCapture);
  $('#hotkey').addEventListener('keydown', event => {
    if (!capturing || event.key === 'Tab') return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === 'Escape') { cancelCapture(); return; }
    if (['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) return;
    if (!(event.ctrlKey || event.altKey || event.metaKey)) return;
    hotkey = [event.ctrlKey && 'Ctrl', event.altKey && 'Alt', event.shiftKey && 'Shift', event.metaKey && 'Win', event.key.toUpperCase()].filter(Boolean).join(' + ');
    cancelCapture();
  });
  $('#centered').addEventListener('change', () => {
    $('#movable').disabled = $('#centered').checked;
    if ($('#centered').checked) {
      $('#movable').checked = false;
      windowElement.style.translate = '';
    }
    $('.titlebar').classList.toggle('movable', $('#movable').checked);
  });
  $('#movable').addEventListener('change', () => $('.titlebar').classList.toggle('movable', $('#movable').checked));
  $('#resizable').addEventListener('change', () => { $('#resize-handle').hidden = !$('#resizable').checked; });

  $('#item-change-icon').addEventListener('click', () => {
    hideContextMenus();
    $('#icon-file').click();
  });
  $('#icon-file').addEventListener('change', event => {
    const file = event.target.files?.[0];
    if (!file || !contextItem) return;
    if (contextItem.customIcon) URL.revokeObjectURL(contextItem.customIcon);
    contextItem.customIcon = URL.createObjectURL(file);
    renderItems();
    event.target.value = '';
  });
  $('#item-restore-icon').addEventListener('click', () => {
    if (contextItem?.customIcon) URL.revokeObjectURL(contextItem.customIcon);
    if (contextItem) contextItem.customIcon = null;
    hideContextMenus();
    renderItems();
  });
  function showMoveMenu(focusFirst = false) {
    const submenu = $('#move-menu');
    submenu.replaceChildren();
    categories.forEach((name, index) => {
      if (index === contextItem?.category) return;
      const button = document.createElement('button');
      button.type = 'button';
      button.role = 'menuitem';
      button.textContent = name;
      button.addEventListener('click', () => {
        contextItem.category = index;
        hideContextMenus();
        renderItems();
      });
      submenu.append(button);
    });
    submenu.hidden = false;
    $('#item-move').setAttribute('aria-expanded', 'true');
    const parent = $('#item-menu');
    const windowRect = windowElement.getBoundingClientRect();
    const parentRect = parent.getBoundingClientRect();
    const moveRect = $('#item-move').getBoundingClientRect();
    const opensLeft = parentRect.right + submenu.offsetWidth > windowRect.right;
    submenu.style.left = `${opensLeft ? parentRect.left - windowRect.left - submenu.offsetWidth + 5 : parentRect.right - windowRect.left - 5}px`;
    submenu.style.top = `${Math.max(4, Math.min(moveRect.top - windowRect.top, windowRect.height - submenu.offsetHeight - 4))}px`;
    if (focusFirst) submenu.querySelector('button')?.focus();
  }
  $('#item-move').addEventListener('pointerenter', () => showMoveMenu(false));
  $('#item-move').addEventListener('click', () => showMoveMenu(true));
  $('#category-new').addEventListener('click', () => {
    hideContextMenus();
    $('#category-name').value = '';
    openDialog('category', contextTrigger || $('#add-button'));
  });
  $('#category-rename').addEventListener('click', () => {
    if (contextCategory == null) return;
    hideContextMenus();
    $('#category-name').value = categories[contextCategory];
    openDialog('category', contextTrigger || $('#add-button'));
    dialog.dataset.rename = String(contextCategory);
    $('#category-title').textContent = '重命名分类';
  });
  $('#category-delete').addEventListener('click', () => {
    if (contextCategory == null || categories.length <= 1) return;
    const destination = contextCategory === 0 ? 1 : contextCategory - 1;
    $('#confirm-message').textContent = `确定删除分类“${categories[contextCategory]}”？分类中的应用不会被删除，将移动到“${categories[destination]}”。`;
    hideContextMenus();
    openDialog('confirm', contextTrigger || $('#add-button'));
  });
  $('#confirm-cancel').addEventListener('click', closeDialog);
  $('#confirm-submit').addEventListener('click', () => {
    const removed = contextCategory;
    const destination = removed === 0 ? 1 : removed - 1;
    const activeBefore = category;
    const removedWasActive = activeBefore === removed;
    items.forEach(item => {
      if (item.category === removed) item.category = destination;
      if (item.category > removed) item.category -= 1;
    });
    categories.splice(removed, 1);
    if (removedWasActive) category = destination > removed ? destination - 1 : destination;
    else if (activeBefore > removed) category = activeBefore - 1;
    search.value = '';
    $('#search-bar').hidden = true;
    $('#search-button').setAttribute('aria-expanded', 'false');
    $('#launcher-view').classList.remove('searching');
    categoryStart = Math.min(categoryStart, categories.length - 1);
    closeDialog();
    renderTabs();
    renderItems();
  });
  $('#item-menu').addEventListener('click', event => {
    const action = event.target.closest('[data-item-action]')?.dataset.itemAction;
    if (!action) return;
    hideContextMenus();
    if (action === 'delete' && contextItem) {
      if (contextItem.customIcon) URL.revokeObjectURL(contextItem.customIcon);
      const index = items.indexOf(contextItem);
      if (index !== -1) items.splice(index, 1);
      renderItems();
    } else if (action === 'sort') {
      items.sort((a, b) => a.category - b.category || a.name.localeCompare(b.name));
      renderItems();
    } else if (['run', 'admin', 'open-with', 'reveal', 'copy'].includes(action) && contextItem) {
      selected = contextItem.id;
      renderItems();
    } else if (action === 'rename' && contextItem) {
      $('#category-name').value = contextItem.name;
      itemRenameMode = true;
      openDialog('category', $('#add-button'));
      $('#category-title').textContent = '重命名项目';
    } else if (action === 'new') {
      openDialog('add', $('#add-button'));
    }
  });

  function bindMenuKeys(menu, options = {}) {
    menu.addEventListener('keydown', event => {
      const buttons = [...menu.querySelectorAll(':scope > button:not(:disabled)')];
      const index = buttons.indexOf(document.activeElement);
      const next = {
        ArrowDown: (index + 1 + buttons.length) % buttons.length,
        ArrowUp: (index - 1 + buttons.length) % buttons.length,
        Home: 0,
        End: buttons.length - 1,
      }[event.key];
      if (next !== undefined && buttons.length) {
        event.preventDefault();
        buttons[next].focus();
      } else if (event.key === 'Escape') {
        event.preventDefault();
        hideContextMenus(true);
      } else if (event.key === 'ArrowRight' && options.submenu) {
        event.preventDefault();
        options.submenu();
      } else if (event.key === 'ArrowLeft' && options.parent) {
        event.preventDefault();
        options.parent.focus();
      }
    });
  }
  bindMenuKeys($('#item-menu'), {submenu: () => showMoveMenu(true)});
  bindMenuKeys($('#category-context-menu'));
  bindMenuKeys($('#move-menu'), {parent: $('#item-move')});

  function openDialog(name, trigger) {
    returnFocus = trigger;
    dialog = $(`#${name}-dialog`);
    $('#overlay').hidden = false;
    dialog.hidden = false;
    $('#app-shell').inert = true;
    setMenu(false);
    setAddMenu(false);
    (name === 'category' ? $('#category-name') : name === 'confirm' ? $('#confirm-submit') : $('#add-file')).focus();
  }
  function closeDialog() {
    if (!dialog) return;
    dialog.hidden = true;
    if (dialog.id === 'category-dialog') {
      delete dialog.dataset.rename;
      itemRenameMode = false;
      $('#category-title').textContent = '新建分类';
    }
    dialog = null;
    $('#overlay').hidden = true;
    $('#app-shell').inert = false;
    returnFocus?.focus();
  }
  function setAddMenu(open) {
    $('#add-menu').hidden = !open;
    $('#add-button').setAttribute('aria-expanded', String(open));
    if (open) { setMenu(false); $('#add-menu-item').focus(); }
  }
  $('#add-button').addEventListener('click', () => setAddMenu($('#add-menu').hidden));
  $('#add-button').addEventListener('keydown', event => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault(); setAddMenu(true);
      if (event.key === 'ArrowUp') $('#add-menu-category').focus();
    }
  });
  $('#add-menu').addEventListener('keydown', event => {
    const buttons = [...$('#add-menu').children];
    const index = buttons.indexOf(document.activeElement);
    const next = {ArrowDown: (index + 1) % 2, ArrowUp: (index + 1) % 2, Home: 0, End: 1}[event.key];
    if (next !== undefined) { event.preventDefault(); buttons[next].focus(); }
    if (event.key === 'Tab') { setAddMenu(false); $('#add-button').focus(); }
  });
  $('#add-menu-item').addEventListener('click', () => openDialog('add', $('#add-button')));
  $('#add-menu-category').addEventListener('click', () => { $('#category-name').value = ''; openDialog('category', $('#add-button')); });
  $('#add-close').addEventListener('click', closeDialog);
  $('#category-cancel').addEventListener('click', closeDialog);
  $('#overlay').addEventListener('click', event => { if (event.target === $('#overlay')) closeDialog(); });
  $('#category-dialog').addEventListener('submit', event => {
    event.preventDefault();
    const name = $('#category-name').value.trim();
    if (!name) return;
    if (itemRenameMode && contextItem) {
      contextItem.name = name;
      closeDialog();
      renderItems();
      return;
    }
    const renameIndex = dialog.dataset.rename;
    if (renameIndex !== undefined) {
      categories[Number(renameIndex)] = name;
      closeDialog();
      renderTabs();
      renderItems();
      return;
    }
    categories.push(name);
    categoryStart = categories.length - 1;
    closeDialog();
    activate(categories.length - 1, true, true);
    $(`#tab-${category}`).focus();
  });
  function addDemoItem(folder) {
    const name = folder ? '示例文件夹' : '示例文件.txt';
    items.push({id: nextItemId++, name, category, icon: folder ? 'explorer' : 'notepad', path: `示例/${name}`});
    closeDialog();
    setSearch(false);
    renderItems();
  }
  $('#add-file').addEventListener('click', () => addDemoItem(false));
  $('#add-folder').addEventListener('click', () => addDemoItem(true));
  $('#drop-zone').addEventListener('dragover', event => { event.preventDefault(); $('#drop-zone').classList.add('dragging'); });
  $('#drop-zone').addEventListener('dragleave', () => $('#drop-zone').classList.remove('dragging'));
  $('#drop-zone').addEventListener('drop', event => {
    event.preventDefault();
    $('#drop-zone').classList.remove('dragging');
    // Deliberately do not inspect dropped files or their contents.
    addDemoItem(false);
  });
  windowElement.addEventListener('dragover', event => event.preventDefault());
  windowElement.addEventListener('drop', event => event.preventDefault());
  function setClosed(value) {
    closed = value;
    $('#app-shell').hidden = value;
    $('#restore-button').hidden = !value;
    $('#resize-handle').hidden = value || !$('#resizable').checked;
    if (value) {
      setSearch(false);
      selected = null;
      setMenu(false);
      setAddMenu(false);
      $('#restore-button').focus();
    } else {
      setSettings(false);
    }
  }
  $('#close-button').addEventListener('click', () => setClosed(true));
  $('#restore-button').addEventListener('click', () => setClosed(false));
  windowElement.addEventListener('keydown', event => {
    if (event.key === 'Escape') {
      event.preventDefault();
      if (dialog) closeDialog();
      else if (!$('#item-menu').hidden || !$('#category-context-menu').hidden) hideContextMenus(true);
      else if (!$('#add-menu').hidden) { setAddMenu(false); $('#add-button').focus(); }
      else if (!$('#category-menu').hidden) { setMenu(false); $('#category-more').focus(); }
      else if (settingsOpen) setSettings(false);
      else if (!$('#search-bar').hidden) { setSearch(false); $('#search-button').focus(); }
      return;
    }
    if (dialog && event.key === 'Tab') {
      const focusable = [...dialog.querySelectorAll('button, input')];
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    }
  });
  // Movement and resizing stay within the webpage; no native window APIs are used.
  let gesture = null;
  $('.titlebar').addEventListener('pointerdown', event => {
    if (event.button !== 0 || event.target.closest('button') || !$('#movable').checked || $('#centered').checked || dialog) return;
    const current = (windowElement.style.translate || '0px 0px').split(' ').map(parseFloat);
    gesture = {kind: 'move', x: event.clientX, y: event.clientY, left: current[0], top: current[1] || 0};
    $('.titlebar').setPointerCapture(event.pointerId);
  });
  $('#resize-handle').addEventListener('pointerdown', event => {
    if (event.button !== 0 || !$('#resizable').checked || closed || dialog) return;
    gesture = {kind: 'resize', x: event.clientX, y: event.clientY, width: windowElement.offsetWidth, height: windowElement.offsetHeight};
    $('#resize-handle').setPointerCapture(event.pointerId);
  });
  windowElement.addEventListener('pointermove', event => {
    if (!gesture) return;
    const dx = event.clientX - gesture.x;
    const dy = event.clientY - gesture.y;
    if (gesture.kind === 'move') {
      windowElement.style.translate = `${Math.max(-12, Math.min(12, gesture.left + dx))}px ${Math.max(-12, Math.min(20, gesture.top + dy))}px`;
    } else {
      windowElement.style.width = `${Math.max(620, Math.min(840, gesture.width + dx))}px`;
      windowElement.style.height = `${Math.max(400, Math.min(620, gesture.height + dy))}px`;
      renderTabs();
    }
  });
  ['pointerup', 'pointercancel', 'lostpointercapture'].forEach(name => windowElement.addEventListener(name, () => { gesture = null; }));
  renderTabs();
  renderItems();
})();
