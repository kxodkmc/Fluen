<template>
  <div :style="outerStyle">
    <div
      :style="panelStyle"
      :class="className"
      role="toolbar"
      :aria-label="orientation === 'vertical' ? 'Vertical application dock' : 'Application dock'"
      @mousemove="handleMouseMove"
      @mouseleave="handleMouseLeave"
    >
      <DockItem
        v-for="(item, index) in items"
        :key="index"
        :onClick="item.onClick"
        :className="item.className"
        :mousePos="mousePos"
        :spring="spring"
        :distance="distance"
        :magnification="magnification"
        :baseItemSize="baseItemSize"
        :orientation="orientation"
        :item="item"
      />
    </div>
  </div>
</template>

<script lang="ts">
import { AnimatePresence, Motion, useMotionValue, useSpring, useTransform } from 'motion-v';
import type { ConcreteComponent, CSSProperties, PropType } from 'vue';
import { computed, defineComponent, h, onMounted, onUnmounted, ref, watch } from 'vue';

const MotionComponent = Motion as unknown as ConcreteComponent;
const AnimatePresenceComponent = AnimatePresence as unknown as ConcreteComponent;

export type SpringOptions = NonNullable<Parameters<typeof useSpring>[1]>;

export type DockOrientation = 'horizontal' | 'vertical';

export type DockItemData = {
  icon: unknown;
  label: unknown;
  onClick: () => void;
  className?: string;
};

export type DockProps = {
  items: DockItemData[];
  className?: string;
  distance?: number;
  panelSize?: number;
  baseItemSize?: number;
  dockSize?: number;
  magnification?: number;
  spring?: SpringOptions;
  orientation?: DockOrientation;
};

/* ── DockIcon ─────────────────────────────────────────────────────────── */
const DockIcon = defineComponent({
  name: 'DockIcon',
  props: {
    className: { type: String, default: '' }
  },
  render() {
    return h(
      'div',
      {
        style: {
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center'
        } satisfies CSSProperties,
        class: this.className
      },
      this.$slots.default?.()
    );
  }
});

/* ── DockLabel ────────────────────────────────────────────────────────── */
const DockLabel = defineComponent({
  name: 'DockLabel',
  props: {
    className: { type: String, default: '' },
    isHovered: {
      type: Object as PropType<ReturnType<typeof useMotionValue<number>>>,
      required: true
    },
    orientation: {
      type: String as PropType<DockOrientation>,
      default: 'horizontal'
    }
  },
  setup(props) {
    const isVisible = ref(false);
    let unsubscribe: (() => void) | null = null;

    onMounted(() => {
      unsubscribe = props.isHovered.on('change', (latest: number) => {
        isVisible.value = latest === 1;
      });
    });

    onUnmounted(() => {
      unsubscribe?.();
    });

    return { isVisible };
  },
  render() {
    const labelStyle: CSSProperties =
      this.orientation === 'vertical'
        ? {
            position: 'absolute',
            left: '100%',
            top: '50%',
            transform: 'translateY(-50%)',
            marginLeft: '0.5rem',
            width: 'fit-content',
            whiteSpace: 'pre',
            borderRadius: '0.375rem',
            border: '1px solid var(--fluen-charcoal)',
            backgroundColor: 'var(--fluen-charcoal)',
            padding: '0.125rem 0.5rem',
            fontSize: '0.75rem',
            color: 'var(--fluen-on-dark)',
            pointerEvents: 'none'
          }
        : {
            position: 'absolute',
            bottom: '100%',
            transform: 'translateX(-50%)',
            marginBottom: '0.5rem',
            width: 'fit-content',
            whiteSpace: 'pre',
            borderRadius: '0.375rem',
            border: '1px solid var(--fluen-charcoal)',
            backgroundColor: 'var(--fluen-charcoal)',
            padding: '0.125rem 0.5rem',
            fontSize: '0.75rem',
            color: 'var(--fluen-on-dark)',
            pointerEvents: 'none'
          };

    return h(AnimatePresenceComponent, {}, () =>
      this.isVisible
        ? [
            h(
              MotionComponent,
              {
                key: 'label',
                as: 'div',
                class: this.className,
                role: 'tooltip',
                style: labelStyle,
                initial: { opacity: 0, y: 0 },
                animate: { opacity: 1, y: -10 },
                exit: { opacity: 0, y: 0 },
                transition: { duration: 0.2 }
              },
              () => this.$slots.default?.()
            )
          ]
        : []
    );
  }
});

/* ── DockItem ─────────────────────────────────────────────────────────── */
const DockItem = defineComponent({
  name: 'DockItem',
  props: {
    className: { type: String, default: '' },
    onClick: { type: Function as PropType<() => void>, default: () => {} },
    mousePos: {
      type: Object as PropType<ReturnType<typeof useMotionValue<number>>>,
      required: true
    },
    spring: { type: Object as PropType<SpringOptions>, required: true },
    distance: { type: Number, required: true },
    baseItemSize: { type: Number, required: true },
    magnification: { type: Number, required: true },
    orientation: {
      type: String as PropType<DockOrientation>,
      default: 'horizontal'
    },
    item: { type: Object as PropType<DockItemData>, required: true }
  },
  setup(props) {
    const itemRef = ref<HTMLDivElement>();
    const isHovered = useMotionValue(0);
    const currentSize = ref(props.baseItemSize);

    const mouseDistance = useTransform(props.mousePos, (val: number) => {
      const rect = itemRef.value?.getBoundingClientRect() ?? { x: 0, y: 0, width: props.baseItemSize };
      const itemPos = props.orientation === 'vertical' ? rect.y : rect.x;
      return val - itemPos - props.baseItemSize / 2;
    });

    const targetSize = useTransform(mouseDistance, (dist: number) => {
      const { baseItemSize, magnification, distance } = props;
      const clamped = Math.max(-distance, Math.min(distance, dist));
      const t = 1 - Math.abs(clamped) / distance;
      return baseItemSize + (magnification - baseItemSize) * t;
    });

    const size = useSpring(targetSize, props.spring);

    watch(
      () => props.baseItemSize,
      newSize => {
        currentSize.value = newSize;
        size.set(newSize);
      }
    );

    let unsubscribeSize: (() => void) | null = null;

    onMounted(() => {
      unsubscribeSize = size.on('change', (latest: number) => {
        currentSize.value = latest;
      });
    });

    onUnmounted(() => {
      unsubscribeSize?.();
    });

    const handleHoverStart = () => isHovered.set(1);
    const handleHoverEnd = () => isHovered.set(0);
    const handleFocus = () => isHovered.set(1);
    const handleBlur = () => isHovered.set(0);

    return {
      itemRef,
      currentSize,
      isHovered,
      handleHoverStart,
      handleHoverEnd,
      handleFocus,
      handleBlur
    };
  },
  render() {
    const icon = typeof this.item.icon === 'function' ? (this.item.icon as () => unknown)() : this.item.icon;
    const label = typeof this.item.label === 'function' ? (this.item.label as () => unknown)() : this.item.label;

    const itemStyle: CSSProperties = {
      width: `${this.currentSize}px`,
      height: `${this.currentSize}px`,
      position: 'relative',
      display: 'inline-flex',
      alignItems: 'center',
      justifyContent: 'center',
      borderRadius: '10px',
      backgroundColor: 'var(--dock-item-bg, var(--fluen-surface))',
      border: '1px solid var(--dock-item-border, var(--fluen-hairline))',
      boxShadow: 'var(--dock-item-shadow, var(--fluen-shadow-card))',
      cursor: 'pointer',
      outline: 'none'
    };

    return h(
      'div',
      {
        ref: 'itemRef',
        style: itemStyle,
        class: this.className,
        tabindex: 0,
        role: 'button',
        'aria-haspopup': 'true',
        onMouseenter: this.handleHoverStart,
        onMouseleave: this.handleHoverEnd,
        onFocus: this.handleFocus,
        onBlur: this.handleBlur,
        onClick: this.onClick
      },
      [
        h(DockIcon, {}, () => [icon]),
        h(DockLabel, { isHovered: this.isHovered, orientation: this.orientation }, () => [typeof label === 'string' ? label : label])
      ]
    );
  }
});

/* ── Dock (主组件) ────────────────────────────────────────────────────── */
export default defineComponent({
  name: 'Dock',
  components: { DockItem },
  props: {
    items: { type: Array as PropType<DockItemData[]>, required: true },
    className: { type: String, default: '' },
    distance: { type: Number, default: 200 },
    panelSize: { type: Number, default: 68 },
    baseItemSize: { type: Number, default: 50 },
    dockSize: { type: Number, default: 256 },
    magnification: { type: Number, default: 70 },
    spring: {
      type: Object as PropType<SpringOptions>,
      default: () => ({ mass: 0.1, stiffness: 150, damping: 12 })
    },
    orientation: {
      type: String as PropType<DockOrientation>,
      default: 'horizontal'
    }
  },
  setup(props) {
    const mousePos = useMotionValue(Infinity);
    const isHovered = useMotionValue(0);
    const currentPanelSize = ref(props.panelSize);

    const maxPanelSize = computed(() => Math.max(props.dockSize, props.magnification + props.magnification / 2 + 4));

    const panelSizeRow = useTransform(isHovered, (hovered: number) =>
      hovered === 1 ? maxPanelSize.value : props.panelSize
    );
    const panelSizeSpring = useSpring(panelSizeRow, props.spring);

    watch([() => props.panelSize, maxPanelSize], () => {
      panelSizeSpring.set(isHovered.get() === 1 ? maxPanelSize.value : props.panelSize);
    });

    let unsubscribePanelSize: (() => void) | null = null;

    onMounted(() => {
      unsubscribePanelSize = panelSizeSpring.on('change', (latest: number) => {
        currentPanelSize.value = latest;
      });
    });

    onUnmounted(() => {
      unsubscribePanelSize?.();
    });

    const handleMouseMove = (event: MouseEvent) => {
      isHovered.set(1);
      mousePos.set(props.orientation === 'vertical' ? event.pageY : event.pageX);
    };

    const handleMouseLeave = () => {
      isHovered.set(0);
      mousePos.set(Infinity);
    };

    const outerStyle = computed<CSSProperties>(() => {
      if (props.orientation === 'vertical') {
        return {
          width: `${currentPanelSize.value}px`,
          height: '100%',
          scrollbarWidth: 'none',
          margin: '0',
          display: 'flex',
          maxHeight: '100%',
          maxWidth: '100%',
          alignItems: 'center',
          justifyContent: 'center'
        };
      }
      return {
        height: `${currentPanelSize.value}px`,
        scrollbarWidth: 'none',
        margin: '0 0.5rem',
        display: 'flex',
        maxWidth: '100%',
        alignItems: 'center'
      };
    });

    const panelStyle = computed<CSSProperties>(() => {
      if (props.orientation === 'vertical') {
        return {
          width: `${props.panelSize}px`,
          height: 'fit-content',
          position: 'absolute',
          top: '50%',
          left: '50%',
          transform: 'translate(-50%, -50%)',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: '0.5rem',
          borderRadius: '1rem',
          backgroundColor: 'var(--dock-bg, var(--fluen-surface))',
          border: '1px solid var(--dock-border, var(--fluen-hairline))',
          padding: '0.5rem 0'
        };
      }
      return {
        height: `${props.panelSize}px`,
        position: 'absolute',
        bottom: '0.5rem',
        left: '50%',
        transform: 'translateX(-50%)',
        display: 'flex',
        alignItems: 'flex-end',
        width: 'fit-content',
        gap: '1rem',
        borderRadius: '1rem',
        backgroundColor: 'var(--dock-bg, var(--fluen-surface))',
        border: '1px solid var(--dock-border, var(--fluen-hairline))',
        padding: '0 0.5rem 0.5rem'
      };
    });

    return {
      mousePos,
      outerStyle,
      panelStyle,
      handleMouseMove,
      handleMouseLeave
    };
  }
});
</script>
