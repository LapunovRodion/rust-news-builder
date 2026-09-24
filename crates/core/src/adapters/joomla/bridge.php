<?php
/**
 * News Builder's bridge into Joomla (002 research R1, R4; contracts/bridge-protocol.md).
 *
 * Sent on stdin to the server's PHP by the News Builder application, run from the Joomla install
 * directory, and gone when it exits: nothing is written to disk. It reads one JSON request from
 * the rest of stdin, boots Joomla's administrator application, does one of three things, and
 * prints one line:
 *
 *     NEWSBUILDER-BRIDGE:{"ok":true,...}
 *
 * - describe: the site's categories, view levels, languages and authors. Writes nothing.
 * - find:     the articles that could be this item's. Writes nothing.
 * - save:     creates or updates one article through com_content's own Article model, so the
 *             asset, the workflow association, the readmore split, the history and the plugins
 *             all happen exactly as they do for an article made in the administrator.
 *
 * There is no operation that removes, trashes or archives an article, and there must never be
 * one (FR-008). crates/core/tests/bridge_no_delete.rs enforces it.
 *
 * Joomla 4 to 6. Database settings come from the site's own configuration.php and are never
 * printed.
 */

// Diagnostics go to stderr; stdout is for the answer line only.
ini_set('display_errors', 'stderr');
error_reporting(E_ALL & ~E_DEPRECATED & ~E_USER_DEPRECATED & ~E_NOTICE);
ob_start();

const NB_SENTINEL = 'NEWSBUILDER-BRIDGE:';
const NB_PROTOCOL = 1;
const NB_MARK = 'newsbuilder:';
/** The pattern com_content's table splits articletext on. */
const NB_READMORE = '#<hr\s+id=("|\')system-readmore("|\')\s*\/*>#i';

/** @param array<string, mixed> $answer */
function nb_answer(array $answer): void
{
    $GLOBALS['nb_answered'] = true;
    while (ob_get_level() > 0) {
        ob_end_clean();
    }
    $json = json_encode(
        $answer,
        JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES | JSON_INVALID_UTF8_SUBSTITUTE
    );
    fwrite(STDOUT, "\n" . NB_SENTINEL . $json . "\n");
    exit(0);
}

/** @param array<string, mixed> $extra */
function nb_fail(string $kind, string $detail, array $extra = []): void
{
    nb_answer(['ok' => false, 'error' => ['kind' => $kind, 'detail' => $detail] + $extra]);
}

set_exception_handler(function (Throwable $e): void {
    nb_fail('exception', get_class($e) . ': ' . $e->getMessage());
});

register_shutdown_function(function (): void {
    if (!empty($GLOBALS['nb_answered'])) {
        return;
    }
    $error = error_get_last();
    if ($error !== null && in_array($error['type'], [E_ERROR, E_PARSE, E_CORE_ERROR, E_COMPILE_ERROR], true)) {
        nb_fail('fatal', $error['message'] . ' in ' . $error['file'] . ':' . $error['line']);
    }
    // Joomla exits on its own when it cannot start, e.g. with no configuration or an
    // installation directory still present. Say so rather than leave the application guessing.
    $said = '';
    while (ob_get_level() > 0) {
        $said = ob_get_clean() . $said;
    }
    nb_fail('boot_failed', 'Joomla stopped before answering: ' . substr(trim(strip_tags($said)), 0, 500));
});

// ---------------------------------------------------------------------------------------------
// The request
// ---------------------------------------------------------------------------------------------

$request = json_decode((string) stream_get_contents(STDIN), true);
if (!is_array($request)) {
    nb_fail('bad_request', 'the request is not a JSON object');
}
if (($request['protocol'] ?? null) !== NB_PROTOCOL) {
    nb_fail('unsupported_protocol', 'this bridge speaks protocol ' . NB_PROTOCOL);
}

// ---------------------------------------------------------------------------------------------
// Booting Joomla (research R4)
// ---------------------------------------------------------------------------------------------

$root = getcwd();
if ($root === false || !is_file($root . '/configuration.php')) {
    nb_fail('boot_failed', 'there is no configuration.php in ' . ($root === false ? 'the current directory' : $root));
}
if (!is_readable($root . '/configuration.php')) {
    nb_fail('boot_failed', 'configuration.php in ' . $root . ' is not readable by this account');
}
if (!is_file($root . '/administrator/includes/framework.php')) {
    nb_fail('boot_failed', $root . ' is not a Joomla installation');
}

// Some of Joomla expects a web request; give it one that names the site.
$siteUrl = isset($request['site_url']) ? (string) $request['site_url'] : 'http://localhost/';
$parts = parse_url($siteUrl) ?: [];
$_SERVER['HTTP_HOST'] = ($parts['host'] ?? 'localhost') . (isset($parts['port']) ? ':' . $parts['port'] : '');
$_SERVER['HTTPS'] = (($parts['scheme'] ?? 'http') === 'https') ? 'on' : 'off';
$_SERVER['REQUEST_METHOD'] = 'POST';
$_SERVER['REQUEST_URI'] = rtrim($parts['path'] ?? '', '/') . '/administrator/index.php';
$_SERVER['SCRIPT_NAME'] = $_SERVER['REQUEST_URI'];

define('_JEXEC', 1);
define('JPATH_BASE', $root . '/administrator');
require_once JPATH_BASE . '/includes/defines.php';
require_once JPATH_BASE . '/includes/framework.php';

$major = (int) JVERSION;
if ($major < 4 || $major > 6) {
    nb_fail('unsupported_joomla', 'Joomla ' . JVERSION . ' is not supported', ['found' => JVERSION]);
}

use Joomla\CMS\Factory;
use Joomla\CMS\Plugin\PluginHelper;

$container = Factory::getContainer();
$container->alias('session', 'session.cli')
    ->alias('JSession', 'session.cli')
    ->alias(\Joomla\CMS\Session\Session::class, 'session.cli')
    ->alias(\Joomla\Session\Session::class, 'session.cli')
    ->alias(\Joomla\Session\SessionInterface::class, 'session.cli');

$app = $container->get(\Joomla\CMS\Application\AdministratorApplication::class);
Factory::$application = $app;
$app->createExtensionNamespaceMap();

$language = $container->get(\Joomla\CMS\Language\LanguageFactoryInterface::class)
    ->createLanguage($app->get('language', 'en-GB'), false);
$app->loadLanguage($language);
$language->load('lib_joomla', JPATH_ADMINISTRATOR);
$language->load('com_content', JPATH_ADMINISTRATOR);

$db = $container->get(\Joomla\Database\DatabaseInterface::class);

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/** A fresh query, on every Joomla from 4 to 6. */
function nb_query($db)
{
    return method_exists($db, 'createQuery') ? $db->createQuery() : $db->getQuery(true);
}

/** @return array{0: string, 1: string} introtext and fulltext, split the way com_content splits. */
function nb_split(string $articletext): array
{
    if (preg_match(NB_READMORE, $articletext) !== 1) {
        return [$articletext, ''];
    }
    $halves = preg_split(NB_READMORE, $articletext, 2);
    return [(string) $halves[0], (string) $halves[1]];
}

/** The ownership mark: which title and text the application last wrote (research R5). */
function nb_mark(string $title, string $introtext, string $fulltext): string
{
    return NB_MARK . hash('sha256', $title . "\n" . $introtext . "\0" . $fulltext);
}

function nb_url(string $siteUrl, int $id, int $catid): string
{
    return rtrim($siteUrl, '/') . '/index.php?option=com_content&view=article&id=' . $id . '&catid=' . $catid;
}

/** An RFC 3339 instant as the UTC wall-clock string Joomla stores. */
function nb_utc(string $instant): string
{
    return (new DateTimeImmutable($instant))
        ->setTimezone(new DateTimeZone('UTC'))
        ->format('Y-m-d H:i:s');
}

/**
 * The request's settings as com_content columns. Only keys the request carries appear, so a
 * setting nobody chose is left to Joomla (FR-012).
 *
 * @param array<string, mixed> $settings
 * @return array<string, mixed>
 */
function nb_columns(array $settings): array
{
    $columns = [];
    if (array_key_exists('category', $settings)) {
        $columns['catid'] = (int) $settings['category'];
    }
    if (array_key_exists('state', $settings)) {
        // Only ever published (1) or unpublished (0): the application has no other state.
        $columns['state'] = $settings['state'] === 'published' ? 1 : 0;
    }
    if (array_key_exists('featured', $settings)) {
        $columns['featured'] = $settings['featured'] ? 1 : 0;
    }
    if (array_key_exists('access', $settings)) {
        $columns['access'] = (int) $settings['access'];
    }
    if (array_key_exists('language', $settings)) {
        $columns['language'] = (string) $settings['language'];
    }
    if (array_key_exists('author', $settings)) {
        $columns['created_by'] = (int) $settings['author'];
    }
    if (array_key_exists('author_alias', $settings)) {
        $columns['created_by_alias'] = (string) $settings['author_alias'];
    }
    if (array_key_exists('publish_up', $settings)) {
        $columns['publish_up'] = nb_utc((string) $settings['publish_up']);
    }
    if (array_key_exists('publish_down', $settings)) {
        $columns['publish_down'] = nb_utc((string) $settings['publish_down']);
    }
    if (array_key_exists('meta_description', $settings)) {
        $columns['metadesc'] = (string) $settings['meta_description'];
    }
    return $columns;
}

/** @return int[] The article's tag ids, sorted. */
function nb_tags_of($db, int $id): array
{
    $ids = $db->setQuery(
        nb_query($db)
            ->select($db->quoteName('tag_id'))
            ->from($db->quoteName('#__contentitem_tag_map'))
            ->where($db->quoteName('type_alias') . ' = ' . $db->quote('com_content.article'))
            ->where($db->quoteName('content_item_id') . ' = ' . $id)
    )->loadColumn();
    $ids = array_map('intval', $ids);
    sort($ids);
    return $ids;
}

/** @return int[] The requested tag ids, sorted. */
function nb_requested_tags(array $settings): array
{
    $ids = array_map('intval', (array) $settings['tags']);
    sort($ids);
    return array_values(array_unique($ids));
}

/** @return array<string, mixed>|null */
function nb_category($db, int $id): ?array
{
    $query = nb_query($db)
        ->select($db->quoteName(['id', 'published']))
        ->from($db->quoteName('#__categories'))
        ->where($db->quoteName('extension') . ' = ' . $db->quote('com_content'))
        ->where($db->quoteName('id') . ' = ' . $id);
    $row = $db->setQuery($query)->loadAssoc();
    return is_array($row) ? $row : null;
}

// ---------------------------------------------------------------------------------------------
// describe
// ---------------------------------------------------------------------------------------------

function nb_describe($db): void
{
    $categories = $db->setQuery(
        nb_query($db)
            ->select($db->quoteName(['id', 'title', 'level', 'published', 'language']))
            ->from($db->quoteName('#__categories'))
            ->where($db->quoteName('extension') . ' = ' . $db->quote('com_content'))
            ->where($db->quoteName('published') . ' IN (0, 1)')
            ->order($db->quoteName('lft'))
    )->loadAssocList();

    $levels = $db->setQuery(
        nb_query($db)
            ->select($db->quoteName(['id', 'title']))
            ->from($db->quoteName('#__viewlevels'))
            ->order($db->quoteName('ordering'))
    )->loadAssocList();

    $languages = $db->setQuery(
        nb_query($db)
            ->select($db->quoteName(['lang_code', 'title']))
            ->from($db->quoteName('#__languages'))
            ->where($db->quoteName('published') . ' = 1')
            ->order($db->quoteName('ordering'))
    )->loadAssocList();

    $authors = $db->setQuery(
        nb_query($db)
            ->select('DISTINCT ' . implode(', ', $db->quoteName(['u.id', 'u.name'])))
            ->from($db->quoteName('#__content', 'c'))
            ->join('INNER', $db->quoteName('#__users', 'u') . ' ON ' . $db->quoteName('u.id') . ' = ' . $db->quoteName('c.created_by'))
            ->where($db->quoteName('u.block') . ' = 0')
            ->order($db->quoteName('u.name'))
    )->loadAssocList();

    // The root of the tag tree is not a tag.
    $tags = $db->setQuery(
        nb_query($db)
            ->select($db->quoteName(['id', 'title', 'level']))
            ->from($db->quoteName('#__tags'))
            ->where($db->quoteName('published') . ' = 1')
            ->where($db->quoteName('level') . ' > 0')
            ->order($db->quoteName('lft'))
    )->loadAssocList();

    nb_answer([
        'ok' => true,
        'joomla_version' => JVERSION,
        'tags' => array_map(function ($t) { return [
            'id' => (int) $t['id'],
            'title' => (string) $t['title'],
            'level' => (int) $t['level'],
        ]; }, $tags),
        'categories' => array_map(function ($c) { return [
            'id' => (int) $c['id'],
            'title' => (string) $c['title'],
            'level' => (int) $c['level'],
            'published' => (int) $c['published'] === 1,
            'language' => (string) $c['language'],
        ]; }, $categories),
        'access_levels' => array_map(function ($l) { return ['id' => (int) $l['id'], 'title' => (string) $l['title']]; }, $levels),
        'languages' => array_merge(
            [['code' => '*', 'title' => 'All']],
            array_map(function ($l) { return ['code' => (string) $l['lang_code'], 'title' => (string) $l['title']]; }, $languages)
        ),
        'authors' => array_map(function ($a) { return ['id' => (int) $a['id'], 'name' => (string) $a['name']]; }, $authors),
    ]);
}

// ---------------------------------------------------------------------------------------------
// find
// ---------------------------------------------------------------------------------------------

/** @param array<string, mixed> $request */
function nb_find($db, array $request): void
{
    $alias = (string) $request['alias'];
    $category = (int) $request['category'];
    $title = (string) $request['title'];
    [$intro, $full] = nb_split((string) $request['articletext']);
    $settings = is_array($request['settings'] ?? null) ? $request['settings'] : [];
    $wanted = nb_columns($settings);
    $siteUrl = (string) ($request['site_url'] ?? '');

    $rows = $db->setQuery(
        nb_query($db)
            ->select($db->quoteName([
                'id', 'catid', 'state', 'title', 'introtext', 'fulltext', 'note', 'featured',
                'access', 'language', 'created_by', 'created_by_alias', 'publish_up',
                'publish_down', 'metadesc', 'images',
            ]))
            ->from($db->quoteName('#__content'))
            ->where($db->quoteName('alias') . ' = ' . $db->quote($alias))
            ->order($db->quoteName('id'))
    )->loadAssocList();

    $articles = [];
    $takenBy = null;
    foreach ($rows as $row) {
        $marked = strpos((string) $row['note'], NB_MARK) === 0;
        if (!$marked) {
            if ((int) $row['catid'] === $category && $takenBy === null) {
                $takenBy = (int) $row['id'];
            }
            continue;
        }

        $matches = $row['note'] === nb_mark((string) $row['title'], (string) $row['introtext'], (string) $row['fulltext']);
        $identical = $row['title'] === $title
            && $row['introtext'] === $intro
            && $row['fulltext'] === $full;
        if (array_key_exists('tags', $settings)) {
            $identical = $identical && nb_tags_of($db, (int) $row['id']) === nb_requested_tags($settings);
        }
        if (array_key_exists('intro_image', $request)) {
            $images = json_decode((string) $row['images'], true);
            $current = is_array($images) ? (string) ($images['image_intro'] ?? '') : '';
            $identical = $identical && $current === (string) $request['intro_image'];
        }
        foreach ($wanted as $column => $value) {
            $current = $row[$column];
            if ($current === null || $value === null) {
                $same = $current === $value;
            } elseif (is_int($value)) {
                $same = (int) $current === $value;
            } else {
                $same = (string) $current === (string) $value;
            }
            $identical = $identical && $same;
        }

        $articles[] = [
            'id' => (int) $row['id'],
            'category' => (int) $row['catid'],
            // Trashed is the one state below -1.
            'trashed' => (int) $row['state'] < -1,
            'content_matches_mark' => $matches,
            'identical' => $identical && (int) $row['state'] >= 0,
            'url' => nb_url($siteUrl, (int) $row['id'], (int) $row['catid']),
        ];
    }

    nb_answer(['ok' => true, 'articles' => $articles, 'alias_taken_by' => $takenBy]);
}

// ---------------------------------------------------------------------------------------------
// save
// ---------------------------------------------------------------------------------------------

/** @param array<string, mixed> $request */
function nb_save($app, $container, $db, array $request): void
{
    $id = isset($request['id']) ? (int) $request['id'] : 0;
    $title = (string) $request['title'];
    $articletext = (string) $request['articletext'];
    [$intro, $full] = nb_split($articletext);
    $settings = is_array($request['settings'] ?? null) ? $request['settings'] : [];
    $columns = nb_columns($settings);
    $siteUrl = (string) ($request['site_url'] ?? '');

    if (isset($columns['catid']) && nb_category($db, $columns['catid']) === null) {
        nb_fail('category_missing', 'category ' . $columns['catid'] . ' does not exist', ['id' => $columns['catid']]);
    }

    if ($id > 0) {
        $exists = (int) $db->setQuery(
            nb_query($db)
                ->select('COUNT(*)')
                ->from($db->quoteName('#__content'))
                ->where($db->quoteName('id') . ' = ' . $id)
        )->loadResult();
        if ($exists === 0) {
            nb_fail('not_found', 'article ' . $id . ' no longer exists');
        }
    }

    // The configured author saves the article, so created_by and modified_by are theirs.
    if (isset($columns['created_by'])) {
        $user = $container->get(\Joomla\CMS\User\UserFactoryInterface::class)->loadUserById($columns['created_by']);
        if ((int) $user->id !== $columns['created_by']) {
            nb_fail('author_missing', 'user ' . $columns['created_by'] . ' does not exist');
        }
        $app->loadIdentity($user);
    }

    foreach (['content', 'finder', 'extension', 'workflow', 'behaviour'] as $group) {
        PluginHelper::importPlugin($group);
    }

    $data = [
        'id' => $id,
        'title' => $title,
        'alias' => (string) $request['alias'],
        'articletext' => $articletext,
        'note' => nb_mark($title, $intro, $full),
    ] + $columns;

    // Tags go through the model's tag handling, so Joomla's map and counters stay right. Only
    // existing tags are sent; an empty list clears them.
    if (array_key_exists('tags', $settings)) {
        $data['tags'] = array_map('strval', nb_requested_tags($settings));
    }

    // The cover (Intro Image). Merged into the article's image settings, so the full-text image
    // and anything else set on that tab in the administrator stay as they are.
    if (array_key_exists('intro_image', $request)) {
        $images = [];
        if ($id > 0) {
            $stored = $db->setQuery(
                nb_query($db)
                    ->select($db->quoteName('images'))
                    ->from($db->quoteName('#__content'))
                    ->where($db->quoteName('id') . ' = ' . $id)
            )->loadResult();
            $decoded = json_decode((string) $stored, true);
            $images = is_array($decoded) ? $decoded : [];
        }
        $cover = (string) $request['intro_image'];
        $images['image_intro'] = $cover;
        $images['image_intro_alt'] = $cover === '' ? '' : $title;
        $data['images'] = $images;
    }

    if ($id === 0) {
        // What the administrator's form gives a new article for anything not chosen.
        $data += [
            'state' => 1,
            'featured' => 0,
            'access' => (int) $app->get('access', 1),
            'language' => '*',
            'metakey' => '',
            'metadesc' => '',
            'images' => [],
            'urls' => [],
            'attribs' => [],
            'metadata' => [],
        ];
    }

    // The front controller defines these while it renders a component; the article model's
    // form loading relies on them.
    if (!defined('JPATH_COMPONENT')) {
        define('JPATH_COMPONENT', JPATH_ADMINISTRATOR . '/components/com_content');
        define('JPATH_COMPONENT_SITE', JPATH_SITE . '/components/com_content');
        define('JPATH_COMPONENT_ADMINISTRATOR', JPATH_ADMINISTRATOR . '/components/com_content');
    }

    $model = $app->bootComponent('com_content')
        ->getMVCFactory()
        ->createModel('Article', 'Administrator', ['ignore_request' => true]);

    if (!$model->save($data)) {
        nb_fail('save_rejected', (string) $model->getError());
    }

    $saved = (int) $model->getState('article.id');
    $catid = (int) $db->setQuery(
        nb_query($db)
            ->select($db->quoteName('catid'))
            ->from($db->quoteName('#__content'))
            ->where($db->quoteName('id') . ' = ' . $saved)
    )->loadResult();

    nb_answer([
        'ok' => true,
        'id' => $saved,
        'created' => $id === 0,
        'url' => nb_url($siteUrl, $saved, $catid),
    ]);
}

// ---------------------------------------------------------------------------------------------

switch ($request['op'] ?? '') {
    case 'describe':
        nb_describe($db);
        break;
    case 'find':
        nb_find($db, $request);
        break;
    case 'save':
        nb_save($app, $container, $db, $request);
        break;
    default:
        nb_fail('bad_request', 'unknown operation');
}
