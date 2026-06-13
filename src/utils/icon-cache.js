/**
 * IconCache - Singleton module for caching SVG icons in IndexedDB.
 *
 * Usage:
 *   await IconCache.init();
 *   await IconCache.put('file_type_rust', '<svg>...</svg>');
 *   var svg = await IconCache.get('file_type_rust');
 *   var exists = await IconCache.has('file_type_rust');
 *   await IconCache.clear();
 */
var IconCache = (function() {
    var DB_NAME = 'tundracode-icon-cache';
    var DB_VERSION = 1;
    var STORE_NAME = 'icons';

    var db = null;

    function openDatabase() {
        return new Promise(function(resolve, reject) {
            var request = indexedDB.open(DB_NAME, DB_VERSION);

            request.onupgradeneeded = function(event) {
                var database = event.target.result;
                if (!database.objectStoreNames.contains(STORE_NAME)) {
                    database.createObjectStore(STORE_NAME);
                }
            };

            request.onsuccess = function(event) {
                db = event.target.result;
                resolve(db);
            };

            request.onerror = function(event) {
                console.error('IconCache: Failed to open database', event.target.error);
                reject(event.target.error);
            };
        });
    }

    function getStore(mode) {
        var tx = db.transaction(STORE_NAME, mode);
        return tx.objectStore(STORE_NAME);
    }

    function promisifyRequest(request) {
        return new Promise(function(resolve, reject) {
            request.onsuccess = function() {
                resolve(request.result);
            };
            request.onerror = function() {
                reject(request.error);
            };
        });
    }

    return {
        /**
         * Initialize the IndexedDB database.
         * Must be called (and awaited) before using other methods.
         */
        init: function() {
            if (db) return Promise.resolve(db);
            return openDatabase();
        },

        /**
         * Retrieve a cached SVG string by key.
         * Returns the SVG string, or null if not found.
         */
        get: function(key) {
            if (!db) {
                return Promise.reject(new Error('IconCache: Call init() first'));
            }
            var store = getStore('readonly');
            var request = store.get(key);
            return promisifyRequest(request);
        },

        /**
         * Store an SVG string in the cache under the given key.
         */
        put: function(key, value) {
            if (!db) {
                return Promise.reject(new Error('IconCache: Call init() first'));
            }
            var store = getStore('readwrite');
            var request = store.put(value, key);
            return promisifyRequest(request);
        },

        /**
         * Check whether a key exists in the cache.
         * Returns true or false.
         */
        has: function(key) {
            if (!db) {
                return Promise.reject(new Error('IconCache: Call init() first'));
            }
            var store = getStore('readonly');
            var request = store.count(key);
            return promisifyRequest(request).then(function(count) {
                return count > 0;
            });
        },

        /**
         * Clear all entries from the cache.
         */
        clear: function() {
            if (!db) {
                return Promise.reject(new Error('IconCache: Call init() first'));
            }
            var store = getStore('readwrite');
            var request = store.clear();
            return promisifyRequest(request);
        }
    };
})();
