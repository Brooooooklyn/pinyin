export enum PINYIN_STYLE {
  // 普通风格，不带声调
  Plain = 0,
  // 带声调的风格, 默认风格
  WithTone = 1,
  // 声调在各个拼音之后，使用数字1-4表示的风格
  WithToneNum = 2,
  // 声调在拼音最后，使用数字1-4表示的风格
  WithToneNumEnd = 3,
  // 首字母风格
  FirstLetter = 4,
}

export function pinyin(input: string): string[]
export function pinyin(
  input: string,
  options: {
    heteronym: true
    style?: PINYIN_STYLE
    segment?: boolean
  },
): string[][]
export function pinyin(
  input: string,
  options: {
    heteronym: false
    style?: PINYIN_STYLE
    segment?: boolean
  },
): string[]
export function pinyin(
  input: string,
  options: {
    heteronym?: boolean
    style?: PINYIN_STYLE
    segment?: boolean
  },
): string[]

export function asyncPinyin(input: string): Promise<string[]>
export function asyncPinyin(
  input: string,
  options: {
    heteronym: true
    style?: PINYIN_STYLE
    segment?: boolean
  },
): Promise<string[][]>
export function asyncPinyin(
  input: string,
  options: {
    heteronym: false
    style?: PINYIN_STYLE
    segment?: boolean
  },
): Promise<string[]>
export function asyncPinyin(
  input: string,
  options: {
    heteronym?: boolean
    style?: PINYIN_STYLE
    segment?: boolean
  },
): Promise<string[]>

export function compare(a: string, b: string): -1 | 0 | 1
