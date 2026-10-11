/**
 * texTile (詞布達意) - 核心交互與雙向智慧連繫引擎
 */

// 狀態機
const state = {
  challenges: [],
  currentIndex: 0,
  mode: 'single', // 'single' | 'checkpoint' | 'scroll'
  evalCache: {}, // 儲存各題評分結果
  currentTilesOrder: [], // 當前詞塊順序
  slotInputs: [] // 縫隙輸入內容 [slot0, slot1, slot2...]
};

// DOM 元素快取
const dom = {
  title: document.getElementById('chal-title'),
  phonetic: document.getElementById('chal-phonetic'),
  category: document.getElementById('chal-cat'),
  hint: document.getElementById('chal-hint'),
  currentNum: document.getElementById('chal-current-num'),
  totalNum: document.getElementById('chal-total-num'),
  btnPrev: document.getElementById('btn-prev'),
  btnNext: document.getElementById('btn-next'),
  weavingTrack: document.getElementById('weaving-track'),
  renderedText: document.getElementById('rendered-text'),
  btnEval: document.getElementById('btn-eval'),
  btnPrevInline: document.getElementById('btn-prev-inline'),
  btnReweave: document.getElementById('btn-reweave'),
  btnNextInline: document.getElementById('btn-next-inline'),
  btnReset: document.getElementById('btn-reset'),
  critiqueCard: document.getElementById('critique-card'),
  scoreNumber: document.getElementById('score-number'),
  verdictSeal: document.getElementById('verdict-seal'),
  critiqueBody: document.getElementById('critique-body'),
  critiqueQuote: document.getElementById('critique-quote'),
  commitStatus: document.getElementById('commit-status')
};

// 1. 初始化與載入題庫
async function init() {
  setupEventListeners();
  try {
    const res = await fetch('/api/challenges');
    if (res.ok) {
      state.challenges = await res.json();
    }
  } catch (e) {
    console.warn("無法連接後端伺服器，載入本機預設題目", e);
  }

  // 離線預設保底題庫
  if (!state.challenges || state.challenges.length === 0) {
    state.challenges = [
      {
        id: "duel_02",
        title: "文膽與唇舌",
        tiles: ["比什麼", "筆舌墨"],
        phonetic: "ㄅㄧˇ ㄕㄣˊ ˙ㄇㄜ / ㄅㄧˇ ㄕㄜˊ ㄇㄛˋ",
        category: "文白對照",
        difficulty: 3,
        hint: "一個是俗語間的較勁攀比，一個是文人書齋裡馳騁的文房三寶。"
      },
      {
        id: "duel_01",
        title: "先機與神仙",
        tiles: ["先這樣", "仙這樣"],
        phonetic: "ㄒㄧㄢ ㄓㄜˋ ㄧㄤˋ",
        category: "音韻雙關",
        difficulty: 2,
        hint: "一個是日常收尾告辭的口頭禪，一個是神仙灑脫超然的風姿。"
      },
      {
        id: "duel_03",
        title: "法度與力量",
        tiles: ["權力", "權利"],
        phonetic: "ㄑㄩㄢˊ ㄌㄧˋ",
        category: "法政語境",
        difficulty: 2,
        hint: "一個是支配統御的強制力量，一個是法律保障的正當利益。"
      }
    ];
  }

  dom.totalNum.textContent = state.challenges.length;
  loadChallenge(0);
}

// 2. 載入特定題目 (首尾自然環狀相接)
function loadChallenge(index) {
  const total = state.challenges.length;
  if (total === 0) return;

  // 環狀首尾相接
  const cyclicIndex = (index % total + total) % total;
  state.currentIndex = cyclicIndex;
  const item = state.challenges[cyclicIndex];

  dom.currentNum.textContent = cyclicIndex + 1;
  dom.title.textContent = item.title;
  dom.phonetic.textContent = item.phonetic;
  dom.category.textContent = item.category;
  dom.hint.textContent = item.hint;

  // 首尾相接，永遠保持可航向上一題與下一題
  dom.btnPrev.disabled = false;
  dom.btnNext.disabled = false;

  // 初始化詞塊與插槽
  state.currentTilesOrder = [...item.tiles];
  state.slotInputs = new Array(state.currentTilesOrder.length + 1).fill("");

  // 若曾快取過該題輸入，則恢復
  if (state.evalCache[item.id]) {
    const saved = state.evalCache[item.id];
    state.currentTilesOrder = [...saved.tilesOrder];
    state.slotInputs = [...saved.slotInputs];
    showCritique(saved.evalResult, saved.commitResult);
  } else {
    hideCritique();
  }

  renderStage();
  updateLivePreview();
}

// 3. 渲染舞台（動態插槽與詞塊磁磚）
function renderStage() {
  dom.weavingTrack.innerHTML = '';

  const numTiles = state.currentTilesOrder.length;
  for (let i = 0; i <= numTiles; i++) {
    // 渲染插槽 (Slot i)
    const slotWrap = document.createElement('div');
    slotWrap.className = 'weaving-slot';
    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'slot-input';
    input.dataset.slotIndex = i;
    input.value = state.slotInputs[i] || "";
    input.placeholder = i === 0 ? "起首織句..." : (i === numTiles ? "收尾拓印..." : "夾道銜接...");

    // 動態自適應寬度
    const adjustWidth = (el) => {
      const len = el.value.length;
      if (len === 0) {
        el.style.width = "110px";
      } else {
        // 中文字約 18px，加上 padding
        el.style.width = Math.min(480, Math.max(110, len * 20 + 36)) + "px";
      }
    };
    adjustWidth(input);

    input.addEventListener('input', (e) => {
      state.slotInputs[i] = e.target.value;
      adjustWidth(e.target);
      updateLivePreview();
    });

    // 支援快速流：Enter 評分
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        dom.btnEval.click();
      }
    });

    slotWrap.appendChild(input);
    dom.weavingTrack.appendChild(slotWrap);

    // 渲染詞塊 (Tile i)（最後一個 slot 之後無 tile）
    if (i < numTiles) {
      const tileText = state.currentTilesOrder[i];
      const tileEl = document.createElement('div');
      tileEl.className = 'tile-item';
      tileEl.draggable = true;
      tileEl.dataset.tileIndex = i;
      tileEl.innerHTML = `
        <span class="tile-drag-handle">⠿</span>
        <span class="tile-text">${tileText}</span>
      `;

      // 拖放事件支援
      setupDragAndDrop(tileEl, i);

      // 快捷點擊交換支援（鍵盤流／點擊流）
      tileEl.title = "拖曳或點擊可交換前後順序";
      tileEl.addEventListener('dblclick', () => {
        swapTiles(i, (i + 1) % numTiles);
      });

      dom.weavingTrack.appendChild(tileEl);
    }
  }
}

// 4. 詞塊拖放與順序切換
let draggedTileIdx = null;

function setupDragAndDrop(tileEl, index) {
  tileEl.addEventListener('dragstart', (e) => {
    draggedTileIdx = index;
    tileEl.classList.add('dragging');
    e.dataTransfer.effectAllowed = 'move';
  });

  tileEl.addEventListener('dragend', () => {
    tileEl.classList.remove('dragging');
    draggedTileIdx = null;
  });

  tileEl.addEventListener('dragover', (e) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
  });

  tileEl.addEventListener('drop', (e) => {
    e.preventDefault();
    if (draggedTileIdx !== null && draggedTileIdx !== index) {
      swapTiles(draggedTileIdx, index);
    }
  });
}

function swapTiles(idx1, idx2) {
  const temp = state.currentTilesOrder[idx1];
  state.currentTilesOrder[idx1] = state.currentTilesOrder[idx2];
  state.currentTilesOrder[idx2] = temp;
  renderStage();
  updateLivePreview();
}

// 5. 即時宣紙拓印渲染預覽
function updateLivePreview() {
  let fullHtml = "";
  let plainText = "";

  const numTiles = state.currentTilesOrder.length;
  for (let i = 0; i <= numTiles; i++) {
    const textPart = state.slotInputs[i] || "";
    if (textPart) {
      fullHtml += `<span>${escapeHtml(textPart)}</span>`;
      plainText += textPart;
    }
    if (i < numTiles) {
      const tileWord = state.currentTilesOrder[i];
      fullHtml += `<mark class="tile-mark">【${escapeHtml(tileWord)}】</mark>`;
      plainText += tileWord;
    }
  }

  dom.renderedText.innerHTML = fullHtml || `<span style="color: #A09B90; font-style: italic;">請在上方織布插槽中填入文句，連結【${state.currentTilesOrder.join('】與【')}】...</span>`;
  state.currentSentence = plainText;
}

// 6. 送出評分與雙向賦能
async function handleEvaluate() {
  const currentChal = state.challenges[state.currentIndex];
  const sentence = state.currentSentence || "";

  // 驗證是否兩/三個詞都已出現
  const missing = state.currentTilesOrder.filter(t => !sentence.includes(t));
  if (missing.length > 0) {
    alert(`尚未完整織入所有詞塊！缺少：${missing.join('、')}`);
    return;
  }

  dom.btnEval.disabled = true;
  dom.btnEval.innerHTML = `<span>⏳ 文心鑑賞中...</span>`;

  try {
    const res = await fetch('/api/evaluate', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        challenge_id: currentChal.id,
        title: currentChal.title,
        tiles: state.currentTilesOrder,
        sentence: sentence
      })
    });

    const evalData = await res.json();
    let commitData = null;

    // 80分門檻防火牆驗證：合格則自動提交賦能
    if (evalData.score >= 80) {
      try {
        const commitRes = await fetch('/api/commit', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            challenge_id: currentChal.id,
            score: evalData.score,
            sentence: sentence,
            tiles: state.currentTilesOrder,
            critique: evalData.critique,
            pithy_quote: evalData.pithy_quote || sentence,
            transitions: evalData.transitions || []
          })
        });
        commitData = await commitRes.json();
      } catch (commitErr) {
        console.warn("自動提交失敗", commitErr);
      }
    }

    // 快取本題成果
    state.evalCache[currentChal.id] = {
      tilesOrder: [...state.currentTilesOrder],
      slotInputs: [...state.slotInputs],
      evalResult: evalData,
      commitResult: commitData
    };

    showCritique(evalData, commitData);

  } catch (err) {
    alert("鑑賞服務暫時不可用，請確認 python server.py 是否啟動。");
    console.error(err);
  } finally {
    dom.btnEval.disabled = false;
    dom.btnEval.innerHTML = `<span>織句鑑賞</span> <span>✦</span>`;
  }
}

// 7. 顯示評分卡片
function showCritique(evalData, commitData) {
  dom.scoreNumber.textContent = evalData.score;
  dom.verdictSeal.textContent = evalData.verdict || (evalData.score >= 80 ? "錦心繡口" : "尚待推敲");
  dom.critiqueBody.textContent = evalData.critique;
  dom.critiqueQuote.textContent = `「${evalData.pithy_quote || state.currentSentence}」`;

  // 雙向賦能狀態條
  if (evalData.score >= 80) {
    dom.verdictSeal.style.borderColor = "var(--accent-cinnabar)";
    dom.verdictSeal.style.color = "var(--accent-cinnabar)";
    dom.commitStatus.className = "commit-status-box commit-status-success";
    dom.commitStatus.innerHTML = `
      <span>🏛️</span>
      <span><strong>雙向賦能成功！</strong> 已自動為【如喜輸入法】建立消歧神經，並將金句典藏至【字戀接龍故事庫】！</span>
    `;
    // 隱藏原鑑賞鈕，顯露「前題回溯」、「逆轉對調」與「邁向下一錦」三位一體導航
    dom.btnEval.style.display = 'none';
    dom.btnPrevInline.style.display = 'inline-flex';
    dom.btnReweave.style.display = 'inline-flex';
    dom.btnNextInline.style.display = 'inline-flex';
  } else {
    dom.verdictSeal.style.borderColor = "var(--ink-muted)";
    dom.verdictSeal.style.color = "var(--ink-muted)";
    dom.commitStatus.className = "commit-status-box commit-status-warning";
    dom.commitStatus.innerHTML = `
      <span>🛡️</span>
      <span>未達 80 分門檻（防污染機制啟動）。請再推敲上下文、增加情境修辭後重新鑑賞！</span>
    `;
    dom.btnEval.style.display = 'inline-flex';
    dom.btnPrevInline.style.display = 'none';
    dom.btnReweave.style.display = 'none';
    dom.btnNextInline.style.display = 'none';
  }

  dom.critiqueCard.classList.add('show');
}

function hideCritique() {
  dom.critiqueCard.classList.remove('show');
  dom.btnEval.style.display = 'inline-flex';
  dom.btnPrevInline.style.display = 'none';
  dom.btnReweave.style.display = 'none';
  dom.btnNextInline.style.display = 'none';
}

// 工具：防 XSS
function escapeHtml(str) {
  return str.replace(/[&<>'"]/g, 
    tag => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#39;', '"': '&quot;' }[tag] || tag)
  );
}

// 8. 綁定全域事件
function setupEventListeners() {
  dom.btnPrev.addEventListener('click', () => loadChallenge(state.currentIndex - 1));
  dom.btnNext.addEventListener('click', () => loadChallenge(state.currentIndex + 1));

  // 前一題回溯 (底部即時導航)
  dom.btnPrevInline.addEventListener('click', () => {
    loadChallenge(state.currentIndex - 1);
  });

  // 邁向下一錦 (底部即時導航)
  dom.btnNextInline.addEventListener('click', () => {
    loadChallenge(state.currentIndex + 1);
  });

  dom.btnReset.addEventListener('click', () => {
    state.slotInputs = new Array(state.currentTilesOrder.length + 1).fill("");
    renderStage();
    updateLivePreview();
    hideCritique();
  });

  // ⟲ 順序對調，再織一錦
  dom.btnReweave.addEventListener('click', () => {
    state.currentTilesOrder.reverse();
    state.slotInputs = new Array(state.currentTilesOrder.length + 1).fill("");
    renderStage();
    updateLivePreview();
    hideCritique();

    // 自動 focus 第一個輸入框
    setTimeout(() => {
      const firstInput = dom.weavingTrack.querySelector('.slot-input');
      if (firstInput) firstInput.focus();
    }, 50);
  });

  dom.btnEval.addEventListener('click', handleEvaluate);

  // 模式切換按鈕
  document.querySelectorAll('.mode-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      document.querySelectorAll('.mode-btn').forEach(b => b.classList.remove('active'));
      e.target.classList.add('active');
      state.mode = e.target.dataset.mode;
    });
  });

  // 快捷鍵：Alt+R 重置，Ctrl+Enter 鑑賞
  window.addEventListener('keydown', (e) => {
    if (e.ctrlKey && e.key === 'Enter') {
      dom.btnEval.click();
    }
  });
}

// 啟動
window.addEventListener('DOMContentLoaded', init);
