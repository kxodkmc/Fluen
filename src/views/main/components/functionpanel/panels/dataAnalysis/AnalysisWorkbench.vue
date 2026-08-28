<script setup lang="ts">
/**
 * AnalysisWorkbench — 分析工作台。
 *
 * 按分析类别（描述统计 / 均值比较 / 列联表 / 正态性 / 相关 / 回归 /
 * 多变量 / 事后检验 / 析因方差分析）组织参数配置与执行入口，
 * 并根据数据类型（问卷 / 实验）标注推荐类别。
 * 分析成功后向父组件发送结果，由「分析结果」标签页渲染。
 */
import { ref, computed, watch } from 'vue';
import { useI18n } from '../../../../../../i18n';
import {
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
} from '../../../../../../composables/useDataAnalysis';
import type {
  CorrelationMethod,
  DatasetKind,
  DatasetSchema,
  KsTestTypeParam,
  PcaMatrix,
  PostHocMethod,
  SsType,
} from '../../../../../../types/dataAnalysis';

const props = defineProps<{
  path: string;
  schema: DatasetSchema;
  /** 数据用途，用于推荐分析类别。 */
  kind: DatasetKind;
}>();

const emit = defineEmits<{ (e: 'ran', type: string, result: unknown): void }>();
const { t } = useI18n();

// --- 选择状态 ---
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
const ksType = ref<KsTestTypeParam>({ kind: 'lilliefors' });
const ksMean = ref(0);
const ksStdDev = ref(1);
const selectedFactors = ref<string[]>([]);
const ssType = ref<SsType>('type_ii');
const analyzing = ref(false);
const error = ref('');

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

/** 按数据类型推荐的分析类别。 */
const recommended: Record<DatasetKind, TabId[]> = {
  questionnaire: ['descriptive', 'correlation', 'regression', 'multivariate'],
  experiment: ['means', 'normality', 'posthoc', 'factorialAnova'],
};

const kindTip = computed(() =>
  props.kind === 'questionnaire'
    ? t('main.sidebar.data.tipQuestionnaire')
    : t('main.sidebar.data.tipExperiment'),
);

const isNumeric = computed(() => {
  const v = props.schema.variables.find((x) => x.name === selectedVar.value);
  return v?.data_type === 'Numeric';
});

const numericVars = computed(() =>
  props.schema.variables.filter((v) => v.data_type === 'Numeric'),
);

const allVars = computed(() => props.schema.variables);

function clearResult() {
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

watch(activeTab, resetSelections);

// --- 执行辅助 ---
async function run(fn: () => Promise<unknown>, type: string) {
  analyzing.value = true;
  clearResult();
  try {
    const result = await fn();
    emit('ran', type, result);
  } catch (err) {
    error.value = String(err);
  } finally {
    analyzing.value = false;
  }
}

// --- 描述统计 ---
const runDescriptive = () => run(() => descriptive(props.path, selectedVar.value), 'descriptive');
const runFrequencies = () => run(() => frequencies(props.path, selectedVar.value), 'frequencies');
const runCrosstab = () => run(() => crosstab(props.path, selectedVar.value, selectedGroupVar.value), 'crosstab');

// --- 均值比较 ---
const runIndTTest = () => run(() => independentTTest(props.path, selectedDepVar.value, selectedGroupVar.value), 'independentTTest');
const runPairedTTest = () => run(() => pairedTTest(props.path, selectedVar1.value, selectedVar2.value), 'pairedTTest');
const runAnova = () => run(() => oneWayAnova(props.path, selectedDepVar.value, selectedFactorVar.value), 'oneWayAnova');
const runMannWhitney = () => run(() => mannWhitneyUTest(props.path, selectedDepVar.value, selectedGroupVar.value), 'mannWhitney');
const runWilcoxon = () => run(() => wilcoxonSignedRankTest(props.path, selectedVar1.value, selectedVar2.value), 'wilcoxon');
const runKruskal = () => run(() => kruskalWallisTest(props.path, selectedDepVar.value, selectedFactorVar.value), 'kruskalWallis');

// --- 列联表 ---
const runChiSquare = () => run(() => chiSquareTest(props.path, selectedVar1.value, selectedVar2.value), 'chiSquare');
const runFisher = () => run(() => fisherExactTest(props.path, selectedVar1.value, selectedVar2.value), 'fisherExact');

// --- 正态性检验 ---
const runShapiro = () => run(() => shapiroWilk(props.path, selectedVar.value), 'shapiroWilk');
const runKs = () => {
  const tp: KsTestTypeParam = ksType.value.kind === 'onesample'
    ? { kind: 'onesample', mean: ksMean.value, std_dev: ksStdDev.value }
    : { kind: 'lilliefors' };
  return run(() => ksNormalityTest(props.path, selectedVar.value, tp), 'ksNormality');
};

// --- 相关分析 ---
const runCorrelation = () => run(() => correlation(props.path, selectedVars.value, corrMethod.value), 'correlation');
const runCorrelationPair = () => run(() => correlationPair(props.path, selectedVar1.value, selectedVar2.value, corrMethod.value), 'correlationPair');
const runPartialCorr = () => run(() => partialCorrelation(props.path, selectedVar1.value, selectedVar2.value, selectedControlVars.value, corrMethod.value), 'partialCorrelation');

// --- 回归 ---
const runRegression = () => run(() => regression(props.path, selectedDepVar.value, selectedVars.value), 'regression');
const runLogistic = () => run(() => logisticRegression(props.path, selectedDepVar.value, selectedVars.value), 'logisticRegression');
const runVif = () => run(() => vif(props.path, selectedVars.value), 'vif');

// --- 多变量 ---
const runPca = () => run(() => pca(props.path, selectedVars.value, selectedMatrix.value), 'pca');
const runReliability = () => run(() => reliability(props.path, selectedVars.value), 'reliability');

// --- 事后检验 ---
const runPostHoc = () => run(() => postHoc(props.path, selectedDepVar.value, selectedFactorVar.value, postHocMethod.value), 'postHoc');

// --- 析因方差分析 ---
const runFactorialAnova = () => run(() => factorialAnova(props.path, selectedDepVar.value, selectedFactors.value, ssType.value), 'factorialAnova');

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
  <div class="awb">
    <!-- 类别提示 -->
    <p class="awb__tip">{{ kindTip }}</p>

    <!-- 类别标签 -->
    <div class="awb-tabs">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        class="awb-tabs__btn"
        :class="{ 'awb-tabs__btn--active': activeTab === tab.id }"
        @click="activeTab = tab.id"
      >
        {{ tab.label }}
        <span v-if="recommended[kind].includes(tab.id)" class="awb-tabs__dot" />
      </button>
    </div>

    <!-- 参数区 -->
    <div class="awb__config">
      <!-- 描述统计 -->
      <template v-if="activeTab === 'descriptive'">
        <p class="awb-hint">{{ t('main.sidebar.data.hintDesc') }}</p>
        <div class="awb-btn-row">
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
        <label class="awb-select">
          {{ t('main.sidebar.data.groupVar') }}
          <select v-model="selectedGroupVar" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
      </template>

      <!-- 均值比较 -->
      <template v-else-if="activeTab === 'means'">
        <p class="awb-hint">{{ t('main.sidebar.data.hintMeans') }}</p>
        <div class="awb-btn-grid">
          <button :disabled="!selectedDepVar || !selectedGroupVar || analyzing" @click="runIndTTest">{{ t('main.sidebar.data.independentTTest') }}</button>
          <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runPairedTTest">{{ t('main.sidebar.data.pairedTTest') }}</button>
          <button :disabled="!selectedDepVar || !selectedFactorVar || analyzing" @click="runAnova">{{ t('main.sidebar.data.oneWayAnova') }}</button>
          <button :disabled="!selectedDepVar || !selectedGroupVar || analyzing" @click="runMannWhitney">{{ t('main.sidebar.data.mannWhitney') }}</button>
          <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runWilcoxon">{{ t('main.sidebar.data.wilcoxon') }}</button>
          <button :disabled="!selectedDepVar || !selectedFactorVar || analyzing" @click="runKruskal">{{ t('main.sidebar.data.kruskalWallis') }}</button>
        </div>
        <label class="awb-select">{{ t('main.sidebar.data.depVar') }}
          <select v-model="selectedDepVar" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
        <label class="awb-select">{{ t('main.sidebar.data.groupVar') }}
          <select v-model="selectedGroupVar" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
        <label class="awb-select">{{ t('main.sidebar.data.factorVar') }}
          <select v-model="selectedFactorVar" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
        <label class="awb-select">{{ t('main.sidebar.data.var1') }}
          <select v-model="selectedVar1" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
        <label class="awb-select">{{ t('main.sidebar.data.var2') }}
          <select v-model="selectedVar2" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
      </template>

      <!-- 列联表 -->
      <template v-else-if="activeTab === 'contingency'">
        <p class="awb-hint">{{ t('main.sidebar.data.hintContingency') }}</p>
        <div class="awb-btn-row">
          <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runChiSquare">{{ t('main.sidebar.data.chiSquare') }}</button>
          <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runFisher">{{ t('main.sidebar.data.fisherExact') }}</button>
        </div>
        <label class="awb-select">{{ t('main.sidebar.data.var1') }}
          <select v-model="selectedVar1" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
        <label class="awb-select">{{ t('main.sidebar.data.var2') }}
          <select v-model="selectedVar2" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
      </template>

      <!-- 正态性检验 -->
      <template v-else-if="activeTab === 'normality'">
        <p class="awb-hint">{{ t('main.sidebar.data.hintNormality') }}</p>
        <div class="awb-btn-row">
          <button :disabled="!isNumeric || analyzing" @click="runShapiro">{{ t('main.sidebar.data.shapiroWilk') }}</button>
          <button :disabled="!isNumeric || analyzing" @click="runKs">{{ t('main.sidebar.data.ksTest') }}</button>
        </div>
        <div class="awb-radio-group">
          <label><input type="radio" :checked="ksType.kind === 'lilliefors'" @change="ksType = { kind: 'lilliefors' }"> {{ t('main.sidebar.data.ksLilliefors') }}</label>
          <label><input type="radio" :checked="ksType.kind === 'onesample'" @change="ksType = { kind: 'onesample', mean: 0, std_dev: 1 }"> {{ t('main.sidebar.data.ksOneSample') }}</label>
        </div>
        <template v-if="ksType.kind === 'onesample'">
          <label class="awb-select">μ <input v-model.number="ksMean" type="number" class="awb-input awb-input--narrow"></label>
          <label class="awb-select">σ <input v-model.number="ksStdDev" type="number" class="awb-input awb-input--narrow"></label>
        </template>
      </template>

      <!-- 相关分析 -->
      <template v-else-if="activeTab === 'correlation'">
        <p class="awb-hint">{{ t('main.sidebar.data.hintCorrelation') }}</p>
        <div class="awb-btn-row">
          <button :disabled="selectedVars.length < 2 || analyzing" @click="runCorrelation">{{ t('main.sidebar.data.correlation') }}</button>
          <button :disabled="!selectedVar1 || !selectedVar2 || analyzing" @click="runCorrelationPair">{{ t('main.sidebar.data.correlationPair') }}</button>
          <button :disabled="!selectedVar1 || !selectedVar2 || selectedControlVars.length < 1 || analyzing" @click="runPartialCorr">{{ t('main.sidebar.data.partialCorrelation') }}</button>
        </div>
        <label class="awb-select">{{ t('main.sidebar.data.method') }}
          <select v-model="corrMethod" class="awb-input">
            <option value="pearson">Pearson</option>
            <option value="spearman">Spearman</option>
            <option value="kendall">Kendall</option>
          </select>
        </label>
        <div class="awb-chiplist">
          <button
            v-for="v in numericVars"
            :key="v.name"
            class="awb-chip"
            :class="{ 'awb-chip--active': selectedVars.includes(v.name) }"
            @click="toggleVar(v.name, 'selectedVars')"
          >{{ v.name }}</button>
        </div>
        <div class="awb-select-row">
          <label class="awb-select">{{ t('main.sidebar.data.var1') }}
            <select v-model="selectedVar1" class="awb-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
          <label class="awb-select">{{ t('main.sidebar.data.var2') }}
            <select v-model="selectedVar2" class="awb-input">
              <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
              <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
            </select>
          </label>
        </div>
        <p class="awb-subhint">{{ t('main.sidebar.data.controlVars') }}</p>
        <div class="awb-chiplist">
          <button
            v-for="v in numericVars"
            :key="v.name"
            class="awb-chip"
            :class="{ 'awb-chip--active': selectedControlVars.includes(v.name) }"
            @click="toggleVar(v.name, 'selectedControlVars')"
          >{{ v.name }}</button>
        </div>
      </template>

      <!-- 回归 -->
      <template v-else-if="activeTab === 'regression'">
        <p class="awb-hint">{{ t('main.sidebar.data.hintRegression') }}</p>
        <div class="awb-btn-row">
          <button :disabled="!selectedDepVar || selectedVars.length < 1 || analyzing" @click="runRegression">{{ t('main.sidebar.data.linearRegression') }}</button>
          <button :disabled="!selectedDepVar || selectedVars.length < 1 || analyzing" @click="runLogistic">{{ t('main.sidebar.data.logisticRegression') }}</button>
          <button :disabled="selectedVars.length < 2 || analyzing" @click="runVif">{{ t('main.sidebar.data.vif') }}</button>
        </div>
        <label class="awb-select">{{ t('main.sidebar.data.depVar') }}
          <select v-model="selectedDepVar" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
        <p class="awb-subhint">{{ t('main.sidebar.data.indepVars') }}</p>
        <div class="awb-chiplist">
          <button
            v-for="v in numericVars"
            :key="v.name"
            class="awb-chip"
            :class="{ 'awb-chip--active': selectedVars.includes(v.name) }"
            @click="toggleVar(v.name, 'selectedVars')"
          >{{ v.name }}</button>
        </div>
      </template>

      <!-- 多变量 -->
      <template v-else-if="activeTab === 'multivariate'">
        <p class="awb-hint">{{ t('main.sidebar.data.hintMultivariate') }}</p>
        <div class="awb-btn-row">
          <button :disabled="selectedVars.length < 2 || analyzing" @click="runPca">{{ t('main.sidebar.data.pca') }}</button>
          <button :disabled="selectedVars.length < 2 || analyzing" @click="runReliability">{{ t('main.sidebar.data.reliability') }}</button>
        </div>
        <label class="awb-select">{{ t('main.sidebar.data.matrix') }}
          <select v-model="selectedMatrix" class="awb-input">
            <option value="correlation">{{ t('main.sidebar.data.matrixCorrelation') }}</option>
            <option value="covariance">{{ t('main.sidebar.data.matrixCovariance') }}</option>
          </select>
        </label>
        <p class="awb-subhint">{{ t('main.sidebar.data.selectVarsMulti') }}</p>
        <div class="awb-chiplist">
          <button
            v-for="v in numericVars"
            :key="v.name"
            class="awb-chip"
            :class="{ 'awb-chip--active': selectedVars.includes(v.name) }"
            @click="toggleVar(v.name, 'selectedVars')"
          >{{ v.name }}</button>
        </div>
      </template>

      <!-- 事后检验 -->
      <template v-else-if="activeTab === 'posthoc'">
        <p class="awb-hint">{{ t('main.sidebar.data.hintPosthoc') }}</p>
        <div class="awb-btn-row">
          <button :disabled="!selectedDepVar || !selectedFactorVar || analyzing" @click="runPostHoc">{{ t('main.sidebar.data.postHoc') }}</button>
        </div>
        <label class="awb-select">{{ t('main.sidebar.data.method') }}
          <select v-model="postHocMethod" class="awb-input">
            <option value="bonferroni">Bonferroni</option>
            <option value="tukey">Tukey HSD</option>
            <option value="scheffe">Scheffé</option>
          </select>
        </label>
        <label class="awb-select">{{ t('main.sidebar.data.depVar') }}
          <select v-model="selectedDepVar" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
        <label class="awb-select">{{ t('main.sidebar.data.factorVar') }}
          <select v-model="selectedFactorVar" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in allVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
      </template>

      <!-- 析因方差分析 -->
      <template v-else-if="activeTab === 'factorialAnova'">
        <p class="awb-hint">{{ t('main.sidebar.data.hintFactorialAnova') }}</p>
        <div class="awb-btn-row">
          <button :disabled="!selectedDepVar || selectedFactors.length < 2 || analyzing" @click="runFactorialAnova">{{ t('main.sidebar.data.factorialAnova') }}</button>
        </div>
        <label class="awb-select">{{ t('main.sidebar.data.depVar') }}
          <select v-model="selectedDepVar" class="awb-input">
            <option value="">{{ t('main.sidebar.data.selectVar') }}</option>
            <option v-for="v in numericVars" :key="v.name" :value="v.name">{{ v.name }}</option>
          </select>
        </label>
        <label class="awb-select">{{ t('main.sidebar.data.ssType') }}
          <select v-model="ssType" class="awb-input">
            <option value="type_i">{{ t('main.sidebar.data.ssTypeI') }}</option>
            <option value="type_ii">{{ t('main.sidebar.data.ssTypeII') }}</option>
          </select>
        </label>
        <p class="awb-subhint">{{ t('main.sidebar.data.factorsMulti') }}</p>
        <div class="awb-chiplist">
          <button
            v-for="v in allVars"
            :key="v.name"
            class="awb-chip"
            :class="{ 'awb-chip--active': selectedFactors.includes(v.name) }"
            @click="toggleFactor(v.name)"
          >{{ v.name }}</button>
        </div>
      </template>
    </div>

    <!-- 错误 -->
    <p v-if="error" class="awb__error">{{ error }}</p>
  </div>
</template>

<style scoped>
.awb {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.awb__tip {
  margin: 0;
  font-size: 12px;
  color: var(--fluen-stone);
}
.awb-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.awb-tabs__btn {
  position: relative;
  border: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-size: 11px;
  padding: 4px 8px;
  border-radius: 9999px;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}
.awb-tabs__btn:hover:not(.awb-tabs__btn--active) {
  background: var(--fluen-hover);
  border-color: var(--fluen-accent);
}
.awb-tabs__btn--active {
  background: var(--fluen-ink);
  color: var(--fluen-on-primary, #fff);
  border-color: var(--fluen-ink);
}
.awb-tabs__dot {
  position: absolute;
  top: -2px;
  right: -2px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--fluen-accent);
}
.awb__config {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.awb-hint {
  font-size: 12px;
  color: var(--fluen-stone);
  line-height: 1.5;
  margin: 0;
}
.awb-subhint {
  font-size: 11px;
  color: var(--fluen-stone);
  margin: 2px 0 0 0;
}
.awb-btn-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.awb-btn-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
}
.awb__config button {
  border: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-size: 12px;
  padding: 5px 10px;
  border-radius: 9999px;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}
.awb__config button:hover:not(:disabled) {
  background: var(--fluen-hover);
  border-color: var(--fluen-accent);
}
.awb__config button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.awb-select {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  color: var(--fluen-stone);
}
.awb-select-row {
  display: flex;
  gap: 8px;
}
.awb-select-row .awb-select {
  flex: 1;
}
.awb-input {
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  padding: 4px 8px;
  font-size: 12px;
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  flex: 1;
  max-width: 140px;
}
.awb-input--narrow {
  max-width: 70px;
}
.awb-chiplist {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.awb-chip {
  border: 1px solid var(--fluen-hairline);
  background: var(--fluen-surface);
  color: var(--fluen-stone);
  font-size: 11px;
  padding: 3px 8px;
  border-radius: 9999px;
  cursor: pointer;
  transition: all 0.15s ease;
}
.awb-chip--active {
  background: var(--fluen-ink);
  color: var(--fluen-on-primary, #fff);
  border-color: var(--fluen-ink);
}
.awb-radio-group {
  display: flex;
  gap: 12px;
  font-size: 12px;
  color: var(--fluen-stone);
}
.awb-radio-group label {
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
}
.awb__error {
  margin: 0;
  font-size: 12px;
  color: var(--fluen-danger, #e8463a);
  word-break: break-all;
}
</style>
