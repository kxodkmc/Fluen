<script setup lang="ts">
/**
 * DataAnalysisPanel — 数据分析面板。
 *
 * 流程：
 *   1. 加载数据文件（CSV / JSON / .sav）
 *   2. 展示变量列表
 *   3. 按标签页切换分析类别：描述统计 / 均值比较 / 列联表 /
 *      正态性检验 / 相关分析 / 回归 / 多变量 / 事后检验
 *   4. 在每个标签页内选择变量并执行分析
 *   5. 结果由 AnalysisResult 组件渲染
 */
import { ref, computed, watch } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { useI18n } from '../../../../../i18n';
import { useProject } from '../../../../../composables/useProject';
import {
  loadDataset,
  descriptive,
  frequencies,
  crosstab,
  independentTTest,
  pairedTTest,
  oneWayAnova,
  mannWhitneyUTest,
  wilcoxonSignedRankTest,
  kruskalWallisTest,
  chiSquareTest,
  fisherExactTest,
  shapiroWilk,
  ksNormalityTest,
  correlation,
  correlationPair,
  partialCorrelation,
  regression,
  logisticRegression,
  vif,
  pca,
  reliability,
  postHoc,
  factorialAnova,
} from '../../../../../composables/useDataAnalysis';
import type {
  Alternative,
  CorrelationMethod,
  DatasetSchema,
  KsTestTypeParam,
  PcaMatrix,
  PostHocMethod,
  SsType,
} from '../../../../../types/dataAnalysis';
import AnalysisResult from './AnalysisResult.vue';

const { t } = useI18n();
const { currentProject } = useProject();

const projectPath = computed(() => currentProject.value?.project_path ?? '');
const hasProject = computed(() => !!projectPath.value);

// --- 数据状态 ---
const schema = ref<DatasetSchema | null>(null);
const loading = ref(false);
const error = ref('');
const selectedVar = ref('');
const selectedVars = ref<string[]>([]);
const selectedDepVar = ref('');
const selectedGroupVar = ref('');
const selectedVar1 = ref('');
const selectedVar2 = ref('');
const selectedControlVars = ref<string[]>([]);
const selectedFactorVar = ref('');
const selectedMatrix = ref<PcaMatrix>('correlation');
const corrMethod = ref<CorrelationMethod>('pearson');
const postHocMethod = ref<PostHocMethod>('bonferroni');
const fisherAlt = ref<Alternative>('twosided');
const ksType = ref<KsTestTypeParam>({ kind: 'lilliefors' });
const ksMean = ref(0);
const ksStdDev = ref(1);
const selectedFactors = ref<string[]>([]);
const ssType = ref<SsType>('type_ii');
const analyzing = ref(false);
const result = ref<unknown>(null);
const resultType = ref('');

// --- 标签页 ---
type TabId =
  | 'descriptive' | 'means' | 'contingency' | 'normality'
  | 'correlation' | 'regression' | 'multivariate' | 'posthoc' | 'factorialAnova';

const activeTab = ref<TabId>('descriptive');

const tabs = computed(() => [
  { id: 'descriptive' as TabId, label: t('main.sidebar.data.tabDescriptive') },
  { id: 'means' as TabId, label: t('main.sidebar.data.tabMeans') },
  { id: 'contingency' as TabId, label: t('main.sidebar.data.tabContingency') },
  { id: 'normality' as TabId, label: t('main.sidebar.data.tabNormality') },
  { id: 'correlation' as TabId, label: t('main.sidebar.data.tabCorrelation') },
  { id: 'regression' as TabId, label: t('main.sidebar.data.tabRegression') },
  { id: 'multivariate' as TabId, label: t('main.sidebar.data.tabMultivariate') },
  { id: 'posthoc' as TabId, label: t('main.sidebar.data.tabPosthoc') },
  { id: 'factorialAnova' as TabId, label: t('main.sidebar.data.tabFactorialAnova') },
]);

const isNumeric = computed(() => {
  if (!schema.value || !selectedVar.value) return false;
  const v = schema.value.variables.find((x) => x.name === selectedVar.value);
  return v?.data_type === 'Numeric';
});

const numericVars = computed(() =>
  schema.value?.variables.filter((v) => v.data_type === 'Numeric') ?? [],
);

const allVars = computed(() => schema.value?.variables ?? []);

// --- 加载数据 ---
async function pickFile(): Promise<void> {
  if (!hasProject.value) return;
  const selected = await open({
    title: t('main.sidebar.data.load'),
    directory: false,
    multiple: false,
    defaultPath: `${projectPath.value}/data`,
    filters: [{ name: 'Data', extensions: ['csv', 'json', 'sav'] }],
  });
  if (typeof selected === 'string') await load(selected);
}

async function load(path: string): Promise<void> {
  loading.value = true;
  error.value = '';
  clearResult();
  schema.value = null;
  try {
    schema.value = await loadDataset(path);
  } catch (err) {
    error.value = String(err);
  } finally {
    loading.value = false;
  }
}

function clearResult() {
  result.value = null;
  resultType.value = '';
  error.value = '';
}

function resetSelections() {
  selectedVar.value = '';
  selectedVars.value = [];
  selectedDepVar.value = '';
  selectedGroupVar.value = '';
  selectedVar1.value = '';
  selectedVar2.value = '';
  selectedControlVars.value = [];
  selectedFactorVar.value = '';
  selectedFactors.value = [];
  clearResult();
}

watch(activeTab, clearResult);

// --- 辅助 ---
async function run(fn: () => Promise<unknown>, type: string) {
  analyzing.value = true;
  clearResult();
  try {
    result.value = await fn();
    resultType.value = type;
  } catch (err) {
    error.value = String(err);
  } finally {
    analyzing.value = false;
  }
}

const path = computed(() => schema.value?.path ?? '');

// --- 描述统计 ---
const runDescriptive = () => run(() => descriptive(path.value, selectedVar.value), 'descriptive');
const runFrequencies = () => run(() => frequencies(path.value, selectedVar.value), 'frequencies');
const runCrosstab = () => run(() => crosstab(path.value, selectedVar.value, selectedGroupVar.value), 'crosstab');

// --- 均值比较 ---
const runIndTTest = () => run(() => independentTTest(path.value, selectedDepVar.value, selectedGroupVar.value), 'independentTTest');
const runPairedTTest = () => run(() => pairedTTest(path.value, selectedVar1.value, selectedVar2.value), 'pairedTTest');
const runAnova = () => run(() => oneWayAnova(path.value, selectedDepVar.value, selectedFactorVar.value), 'oneWayAnova');
const runMannWhitney = () => run(() => mannWhitneyUTest(path.value, selectedDepVar.value, selectedGroupVar.value), 'mannWhitney');
const runWilcoxon = () => run(() => wilcoxonSignedRankTest(path.value, selectedVar1.value, selectedVar2.value), 'wilcoxon');
const runKruskal = () => run(() => kruskalWallisTest(path.value, selectedDepVar.value, selectedFactorVar.value), 'kruskalWallis');

// --- 列联表 ---
const runChiSquare = () => run(() => chiSquareTest(path.value, selectedVar1.value, selectedVar2.value), 'chiSquare');
const runFisher = () => run(() => fisherExactTest(path.value, selectedVar1.value, selectedVar2.value, fisherAlt.value), 'fisherExact');

// --- 正态性检验 ---
const runShapiro = () => run(() => shapiroWilk(path.value, selectedVar.value), 'shapiroWilk');
const runKs = () => {
  const tp: KsTestTypeParam = ksType.value.kind === 'onesample'
    ? { kind: 'onesample', mean: ksMean.value, std_dev: ksStdDev.value }
    : { kind: 'lilliefors' };
  return run(() => ksNormalityTest(path.value, selectedVar.value, tp), 'ksNormality');
};

// --- 相关分析 ---
const runCorrelation = () => run(() => correlation(path.value, selectedVars.value, corrMethod.value), 'correlation');
const runCorrelationPair = () => run(() => correlationPair(path.value, selectedVar1.value, selectedVar2.value, corrMethod.value), 'correlationPair');
const runPartialCorr = () => run(() => partialCorrelation(path.value, selectedVar1.value, selectedVar2.value, selectedControlVars.value, corrMethod.value), 'partialCorrelation');

// --- 回归 ---
const runRegression = () => run(() => regression(path.value, selectedDepVar.value, selectedVars.value), 'regression');
const runLogistic = () => run(() => logisticRegression(path.value, selectedDepVar.value, selectedVars.value), 'logisticRegression');
const runVif = () => run(() => vif(path.value, selectedVars.value), 'vif');

// --- 多变量 ---
const runPca = () => run(() => pca(path.value, selectedVars.value, selectedMatrix.value), 'pca');
const runReliability = () => run(() => reliability(path.value, selectedVars.value), 'reliability');

// --- 事后检验 ---
const runPostHoc = () => run(() => postHoc(path.value, selectedDepVar.value, selectedFactorVar.value, postHocMethod.value), 'postHoc');

// --- 析因方差分析 ---
const runFactorialAnova = () => run(() => factorialAnova(path.value, selectedDepVar.value, selectedFactors.value, ssType.value), 'factorialAnova');

// --- 多选切换辅助 ---
function toggleVar(name: string, list: 'selectedVars' | 'selectedControlVars') {
  const arr = list === 'selectedVars' ? selectedVars : selectedControlVars;
  const idx = arr.value.indexOf(name);
  if (idx >= 0) arr.value.splice(idx, 1);
  else arr.value.push(name);
}

/** 切换析因 ANOVA 因子选择。 */
function toggleFactor(name: string): void {
  const idx = selectedFactors.value.indexOf(name);
  if (idx >= 0) selectedFactors.value.splice(idx, 1);
  else selectedFactors.value.push(name);
}
</script>

<template>
  <div class="da-panel">
    <!-- 头部 -->
    <div class="da-panel__header">
      <span class="da-panel__title">{{ t('main.sidebar.data.title') }}</span>
      <button class="da-panel__load" :disabled="!hasProject || loading" @click="pickFile">
        {{ t('main.sidebar.data.load') }}
      </button>
    </div>

    <template v-if="!hasProject">
      <p class="da-panel__empty">{{ t('main.sidebar.data.noProject') }}</p>
    </template>

    <template v-else-if="loading">
      <p class="da-panel__empty">{{ t('main.sidebar.data.loadingFile') }}</p>
    </template>

    <template v-else-if="error && !schema">
      <p class="da-panel__error">{{ error }}</p>
    </template>

    <template v-else-if="schema">
      <div class="da-panel__meta">
        <span>{{ t('main.sidebar.data.nRows', { n: schema.n_rows }) }}</span>
        <span>{{ t('main.sidebar.data.nVars', { n: schema.n_vars }) }}</span>
      </div>

      <!-- 变量列表 -->
      <div class="da-panel__vars">
        <button
          v-for="v in allVars"
          :key="v.name"
          class="da-panel__var"
          :class="{ 'da-panel__var--active': v.name === selectedVar }"
          @click="selectedVar = v.name"
        >
          <span class="da-panel__var-name">{{ v.name }}</span>
          <span class="da-panel__var-meta">
            {{ v.data_type === 'Numeric' ? 'N' : 'T' }} · {{ v.n_valid }}/{{ v.n_valid + v.n_missing }}
          </span>
        </button>
      </div>

      <!-- 标签页 -->
      <div class="da-tabs">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          class="da-tabs__btn"
          :class="{ 'da-tabs__btn--active': activeTab === tab.id }"
          @click="activeTab = tab.id; resetSelections()"
        >
          {{ tab.label }}
        </button>
      </div>

      <!-- 参数区 -->
      <div class="da-panel__config">
        <!-- 描述统计 -->
        <template v-if="activeTab === 'descriptive'">
          <p class="da-hint">{{ t('main.sidebar.data.hintDesc') }}</p>
          <div class="da-btn-row">
            <button :disabled="!isNumeric || analyzing" @click="runDescriptive">
              {{ t('main.sidebar.data.descriptive') }}
            </button>
            <button :disabled="!selectedVar || analyzing" @click="runFrequencies">
              {{ t('main.sidebar.data.frequencies') }}
            </button>
            <button :disabled="!selectedVar || !selectedGroupVar || analyzing" @click="runCrosstab">
              {{ t('main.sidebar.data.crosstab') }}
            </button>
          </div>
          <label class="da-select">
            {{ t('main.sidebar.data.groupVar') }}
            <select v-model="selectedGroupVar" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
        </template>

        <!-- 均值比较 -->
        <template v-else-if="activeTab === 'means'">
          <p class="da-hint">{{ t('main.sidebar.data.hintMeans') }}</p>
          <div class="da-btn-grid">
            <button :disabled="!selectedDepVar || !selectedGroupVar || analyzing" @click="runIndTTest">{{ t('main.sidebar.data.independentTTest') }}</button>
            <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runPairedTTest">{{ t('main.sidebar.data.pairedTTest') }}</button>
            <button :disabled="!selectedDepVar || !selectedFactorVar || analyzing" @click="runAnova">{{ t('main.sidebar.data.oneWayAnova') }}</button>
            <button :disabled="!selectedDepVar || !selectedGroupVar || analyzing" @click="runMannWhitney">{{ t('main.sidebar.data.mannWhitney') }}</button>
            <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runWilcoxon">{{ t('main.sidebar.data.wilcoxon') }}</button>
            <button :disabled="!selectedDepVar || !selectedFactorVar || analyzing" @click="runKruskal">{{ t('main.sidebar.data.kruskalWallis') }}</button>
          </div>
          <label class="da-select">{{ t('main.sidebar.data.depVar') }}
            <select v-model="selectedDepVar" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <label class="da-select">{{ t('main.sidebar.data.groupVar') }}
            <select v-model="selectedGroupVar" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <label class="da-select">{{ t('main.sidebar.data.factorVar') }}
            <select v-model="selectedFactorVar" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <label class="da-select">{{ t('main.sidebar.data.var1') }}
            <select v-model="selectedVar1" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <label class="da-select">{{ t('main.sidebar.data.var2') }}
            <select v-model="selectedVar2" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
        </template>

        <!-- 列联表 -->
        <template v-else-if="activeTab === 'contingency'">
          <p class="da-hint">{{ t('main.sidebar.data.hintContingency') }}</p>
          <div class="da-btn-row">
            <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runChiSquare">{{ t('main.sidebar.data.chiSquare') }}</button>
            <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runFisher">{{ t('main.sidebar.data.fisherExact') }}</button>
          </div>
          <label class="da-select">{{ t('main.sidebar.data.var1') }}
            <select v-model="selectedVar1" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <label class="da-select">{{ t('main.sidebar.data.var2') }}
            <select v-model="selectedVar2" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <label class="da-select">{{ t('main.sidebar.data.alternative') }}
            <select v-model="fisherAlt" class="da-input">
              <option value="twosided">{{ t('main.sidebar.data.altTwoSided') }}</option>
              <option value="less">{{ t('main.sidebar.data.altLess') }}</option>
              <option value="greater">{{ t('main.sidebar.data.altGreater') }}</option>
            </select>
          </label>
        </template>

        <!-- 正态性检验 -->
        <template v-else-if="activeTab === 'normality'">
          <p class="da-hint">{{ t('main.sidebar.data.hintNormality') }}</p>
          <div class="da-btn-row">
            <button :disabled="!isNumeric || analyzing" @click="runShapiro">{{ t('main.sidebar.data.shapiroWilk') }}</button>
            <button :disabled="!isNumeric || analyzing" @click="runKs">{{ t('main.sidebar.data.ksTest') }}</button>
          </div>
          <div class="da-radio-group">
            <label><input type="radio" :checked="ksType.kind === 'lilliefors'" @change="ksType = { kind: 'lilliefors' }"> {{ t('main.sidebar.data.ksLilliefors') }}</label>
            <label><input type="radio" :checked="ksType.kind === 'onesample'" @change="ksType = { kind: 'onesample', mean: 0, std_dev: 1 }"> {{ t('main.sidebar.data.ksOneSample') }}</label>
          </div>
          <template v-if="ksType.kind === 'onesample'">
            <label class="da-select">μ <input v-model.number="ksMean" type="number" class="da-input da-input--narrow"></label>
            <label class="da-select">σ <input v-model.number="ksStdDev" type="number" class="da-input da-input--narrow"></label>
          </template>
        </template>

        <!-- 相关分析 -->
        <template v-else-if="activeTab === 'correlation'">
          <p class="da-hint">{{ t('main.sidebar.data.hintCorrelation') }}</p>
          <div class="da-btn-row">
            <button :disabled="selectedVars.length < 2 || analyzing" @click="runCorrelation">{{ t('main.sidebar.data.correlation') }}</button>
            <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runCorrelationPair">{{ t('main.sidebar.data.correlationPair') }}</button>
            <button :disabled="!selectedVar1 || !selectedVar2 || selectedControlVars.length < 1 || analyzing" @click="runPartialCorr">{{ t('main.sidebar.data.partialCorrelation') }}</button>
          </div>
          <label class="da-select">{{ t('main.sidebar.data.method') }}
            <select v-model="corrMethod" class="da-input">
              <option value="pearson">Pearson</option>
              <option value="spearman">Spearman</option>
              <option value="kendall">Kendall</option>
            </select>
          </label>
          <div class="da-chiplist">
            <button
              v-for="v in numericVars"
              :key="v.name"
              class="da-chip"
              :class="{ 'da-chip--active': selectedVars.includes(v.name) }"
              @click="toggleVar(v.name, 'selectedVars')"
            >{{ v.name }}</button>
          </div>
          <div class="da-select-row">
            <label class="da-select">{{ t('main.sidebar.data.var1') }}
              <select v-model="selectedVar1" class="da-input">
                <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
                <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
              </select>
            </label>
            <label class="da-select">{{ t('main.sidebar.data.var2') }}
              <select v-model="selectedVar2" class="da-input">
                <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
                <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
              </select>
            </label>
          </div>
          <p class="da-subhint">{{ t('main.sidebar.data.controlVars') }}</p>
          <div class="da-chiplist">
            <button
              v-for="v in numericVars"
              :key="v.name"
              class="da-chip"
              :class="{ 'da-chip--active': selectedControlVars.includes(v.name) }"
              @click="toggleVar(v.name, 'selectedControlVars')"
            >{{ v.name }}</button>
          </div>
        </template>

        <!-- 回归 -->
        <template v-else-if="activeTab === 'regression'">
          <p class="da-hint">{{ t('main.sidebar.data.hintRegression') }}</p>
          <div class="da-btn-row">
            <button :disabled="!selectedDepVar || selectedVars.length < 1 || analyzing" @click="runRegression">{{ t('main.sidebar.data.linearRegression') }}</button>
            <button :disabled="!selectedDepVar || selectedVars.length < 1 || analyzing" @click="runLogistic">{{ t('main.sidebar.data.logisticRegression') }}</button>
            <button :disabled="selectedVars.length < 2 || analyzing" @click="runVif">{{ t('main.sidebar.data.vif') }}</button>
          </div>
          <label class="da-select">{{ t('main.sidebar.data.depVar') }}
            <select v-model="selectedDepVar" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <p class="da-subhint">{{ t('main.sidebar.data.indepVars') }}</p>
          <div class="da-chiplist">
            <button
              v-for="v in numericVars"
              :key="v.name"
              class="da-chip"
              :class="{ 'da-chip--active': selectedVars.includes(v.name) }"
              @click="toggleVar(v.name, 'selectedVars')"
            >{{ v.name }}</button>
          </div>
        </template>

        <!-- 多变量 -->
        <template v-else-if="activeTab === 'multivariate'">
          <p class="da-hint">{{ t('main.sidebar.data.hintMultivariate') }}</p>
          <div class="da-btn-row">
            <button :disabled="selectedVars.length < 2 || analyzing" @click="runPca">{{ t('main.sidebar.data.pca') }}</button>
            <button :disabled="selectedVars.length < 2 || analyzing" @click="runReliability">{{ t('main.sidebar.data.reliability') }}</button>
          </div>
          <label class="da-select">{{ t('main.sidebar.data.matrix') }}
            <select v-model="selectedMatrix" class="da-input">
              <option value="correlation">{{ t('main.sidebar.data.matrixCorrelation') }}</option>
              <option value="covariance">{{ t('main.sidebar.data.matrixCovariance') }}</option>
            </select>
          </label>
          <p class="da-subhint">{{ t('main.sidebar.data.selectVarsMulti') }}</p>
          <div class="da-chiplist">
            <button
              v-for="v in numericVars"
              :key="v.name"
              class="da-chip"
              :class="{ 'da-chip--active': selectedVars.includes(v.name) }"
              @click="toggleVar(v.name, 'selectedVars')"
            >{{ v.name }}</button>
          </div>
        </template>

        <!-- 事后检验 -->
        <template v-else-if="activeTab === 'posthoc'">
          <p class="da-hint">{{ t('main.sidebar.data.hintPosthoc') }}</p>
          <div class="da-btn-row">
            <button :disabled="!selectedDepVar || !selectedFactorVar || analyzing" @click="runPostHoc">{{ t('main.sidebar.data.postHoc') }}</button>
          </div>
          <label class="da-select">{{ t('main.sidebar.data.method') }}
            <select v-model="postHocMethod" class="da-input">
              <option value="bonferroni">Bonferroni</option>
              <option value="tukey">Tukey HSD</option>
              <option value="scheffe">Scheffé</option>
            </select>
          </label>
          <label class="da-select">{{ t('main.sidebar.data.depVar') }}
            <select v-model="selectedDepVar" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <label class="da-select">{{ t('main.sidebar.data.factorVar') }}
            <select v-model="selectedFactorVar" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
        </template>

        <!-- 析因方差分析 -->
        <template v-else-if="activeTab === 'factorialAnova'">
          <p class="da-hint">{{ t('main.sidebar.data.hintFactorialAnova') }}</p>
          <div class="da-btn-row">
            <button :disabled="!selectedDepVar || selectedFactors.length < 2 || analyzing" @click="runFactorialAnova">{{ t('main.sidebar.data.factorialAnova') }}</button>
          </div>
          <label class="da-select">{{ t('main.sidebar.data.depVar') }}
            <select v-model="selectedDepVar" class="da-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <label class="da-select">{{ t('main.sidebar.data.ssType') }}
            <select v-model="ssType" class="da-input">
              <option value="type_i">{{ t('main.sidebar.data.ssTypeI') }}</option>
              <option value="type_ii">{{ t('main.sidebar.data.ssTypeII') }}</option>
            </select>
          </label>
          <p class="da-subhint">{{ t('main.sidebar.data.factorsMulti') }}</p>
          <div class="da-chiplist">
            <button
              v-for="v in allVars"
              :key="v.name"
              class="da-chip"
              :class="{ 'da-chip--active': selectedFactors.includes(v.name) }"
              @click="toggleFactor(v.name)"
            >{{ v.name }}</button>
          </div>
        </template>
      </div>

      <!-- 错误 -->
      <p v-if="error" class="da-panel__error">{{ error }}</p>

      <!-- 结果 -->
      <AnalysisResult v-if="result" :result="result" :type="resultType" />
    </template>

    <template v-else>
      <p class="da-panel__empty">{{ t('main.sidebar.data.noData') }}</p>
    </template>
  </div>
</template>

<style scoped>
.da-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 12px;
  gap: 10px;
  overflow-y: auto;
}
.da-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.da-panel__title {
  font-size: 13px;
  font-weight: 600;
  color: var(--fluen-ink);
}
.da-panel__load,
.da-panel__config button,
.da-tabs__btn {
  border: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-size: 12px;
  padding: 5px 10px;
  border-radius: 9999px;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}
.da-panel__load:hover:not(:disabled),
.da-panel__config button:hover:not(:disabled),
.da-tabs__btn:hover:not(.da-tabs__btn--active) {
  background: var(--fluen-hover);
  border-color: var(--fluen-accent);
}
.da-panel__load:disabled,
.da-panel__config button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.da-panel__meta {
  display: flex;
  gap: 12px;
  font-size: 12px;
  color: var(--fluen-stone);
}
.da-panel__vars {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.da-panel__var {
  display: flex;
  align-items: center;
  justify-content: space-between;
  border: none;
  background: transparent;
  padding: 6px 8px;
  border-radius: 6px;
  cursor: pointer;
  text-align: left;
}
.da-panel__var:hover {
  background: var(--fluen-hover);
}
.da-panel__var--active {
  background: var(--fluen-hover);
  box-shadow: inset 2px 0 0 var(--fluen-accent);
}
.da-panel__var-name {
  font-size: 13px;
  color: var(--fluen-ink);
}
.da-panel__var-meta {
  font-size: 11px;
  color: var(--fluen-stone);
}
.da-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.da-tabs__btn {
  font-size: 11px;
  padding: 4px 8px;
}
.da-tabs__btn--active {
  background: var(--fluen-ink);
  color: var(--fluen-on-primary, #fff);
  border-color: var(--fluen-ink);
}
.da-panel__config {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.da-hint {
  font-size: 12px;
  color: var(--fluen-stone);
  line-height: 1.5;
  margin: 0;
}
.da-subhint {
  font-size: 11px;
  color: var(--fluen-stone);
  margin: 2px 0 0 0;
}
.da-btn-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.da-btn-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
}
.da-select {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--fluen-stone);
}
.da-select-row {
  display: flex;
  gap: 8px;
}
.da-select-row .da-select {
  flex: 1;
}
.da-input {
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  padding: 4px 8px;
  font-size: 12px;
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  flex: 1;
  max-width: 140px;
}
.da-input--narrow {
  max-width: 70px;
}
.da-chiplist {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.da-chip {
  border: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  color: var(--fluen-stone);
  font-size: 11px;
  padding: 3px 8px;
  border-radius: 9999px;
  cursor: pointer;
  transition: all 0.15s ease;
}
.da-chip--active {
  background: var(--fluen-ink);
  color: var(--fluen-on-primary, #fff);
  border-color: var(--fluen-ink);
}
.da-radio-group {
  display: flex;
  gap: 12px;
  font-size: 12px;
  color: var(--fluen-stone);
}
.da-radio-group label {
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
}
.da-panel__empty {
  font-size: 13px;
  color: var(--fluen-stone);
  text-align: center;
  padding: 16px 0;
}
.da-panel__error {
  font-size: 12px;
  color: var(--fluen-danger, #e8463a);
  word-break: break-all;
}
</style>
