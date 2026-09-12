import { animals, colors, uniqueNamesGenerator } from 'unique-names-generator';
import { getLocale, type Locale } from '#lib/paraglide/runtime.js';

/**
 * Default names for freshly created servers and nodes.
 *
 * A bare type label ("Entry", "エントリー") is useless the moment a canvas holds
 * two of a kind, and users rarely rename right away, so the type label gets a
 * random colour + animal suffix in the active locale. Colours and animals only:
 * the generator's adjective dictionary contains words like `broken` and `wrong`
 * that read as status on an infrastructure canvas.
 *
 * `unique-names-generator` ships English dictionaries only — no maintained npm
 * word list exists for Japanese — so the Japanese words are curated here and
 * fed to the same generator.
 */

/** `〜いろの` forms only, so they glue straight onto a noun. */
const JA_COLORS = [
	'あさぎいろの',
	'あいいろの',
	'うぐいすいろの',
	'きいろの',
	'きんいろの',
	'ぎんいろの',
	'くろいろの',
	'こんいろの',
	'さくらいろの',
	'しゅいろの',
	'しろいろの',
	'そらいろの',
	'ちゃいろの',
	'はいいろの',
	'ふじいろの',
	'べにいろの',
	'みずいろの',
	'みどりいろの',
	'むらさきいろの',
	'もえぎいろの',
	'ももいろの',
	'やまぶきいろの',
	'るりいろの',
	'わかくさいろの'
];

const JA_ANIMALS = [
	'アザラシ',
	'イタチ',
	'イルカ',
	'ウサギ',
	'オオカミ',
	'カピバラ',
	'カメ',
	'カワウソ',
	'キツネ',
	'クジラ',
	'クマ',
	'コアラ',
	'シカ',
	'スズメ',
	'タヌキ',
	'ツバメ',
	'ツル',
	'トカゲ',
	'ハクチョウ',
	'ハヤブサ',
	'パンダ',
	'フクロウ',
	'ペンギン',
	'ホタル',
	'ラッコ',
	'リス',
	'ワシ'
];

const randomPair = (locale: Locale): string =>
	locale === 'ja'
		? uniqueNamesGenerator({ dictionaries: [JA_COLORS, JA_ANIMALS], length: 2, separator: '' })
		: uniqueNamesGenerator({
				dictionaries: [colors, animals],
				length: 2,
				separator: '-',
				style: 'lowerCase'
			});

const NO_NAMES: ReadonlySet<string> = new Set();

/**
 * `<type label> <random pair>`, e.g. `Entry azure-otter` or
 * `エントリー るりいろのカワウソ`. Both dictionaries keep the result far below
 * the control plane's 128-character name limit.
 *
 * The generator draws independently, so `taken` (the names already on the
 * canvas) is used to reject collisions — the Japanese space is only 648 pairs,
 * and a duplicate default is exactly the confusion this is meant to avoid.
 * After a bounded number of draws a counter is appended instead of looping.
 */
export function suggestName(
	typeLabel: string,
	locale: Locale = getLocale(),
	taken: ReadonlySet<string> = NO_NAMES
): string {
	for (let attempt = 0; attempt < 16; attempt += 1) {
		const candidate = `${typeLabel} ${randomPair(locale)}`;
		if (!taken.has(candidate)) return candidate;
	}
	const crowded = `${typeLabel} ${randomPair(locale)}`;
	for (let suffix = 2; ; suffix += 1) {
		const candidate = `${crowded} ${suffix}`;
		if (!taken.has(candidate)) return candidate;
	}
}
