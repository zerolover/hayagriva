// Demonstrates calling hayagriva-ffi from C++: load a bibliography, list
// styles/entries, render citations for a couple of citation groups, and
// group the resulting bibliography items back by "slide".
//
// Usage:
//   hayagriva_cpp_example --bib <bib-file> [--style <style-key>]
//                        [--locale <locale>] --cite <citation-groups-file>

#include <hayagriva.h>

#include "CLI11.hpp"
#include "json.hpp"

#include <cstdio>
#include <fstream>
#include <iostream>
#include <map>
#include <memory>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

using json = nlohmann::json;

namespace {

struct Config {
    bool list_styles_only = false;
    bool list_locales_only = false;
    bool list_entries_only = false;
    std::string bib_path;
    std::string style_key = "apa";
    const char* locale = nullptr;
    std::optional<std::string> locale_storage;
    std::string cite_path;
};

struct CStrDeleter {
    void operator()(char* p) const {
        if (p) hayagriva_free_string(p);
    }
};
using CStrPtr = std::unique_ptr<char, CStrDeleter>;

struct CtxDeleter {
    void operator()(HayagrivaCtx* p) const { hayagriva_free(p); }
};
using CtxPtr = std::unique_ptr<HayagrivaCtx, CtxDeleter>;

// Every hayagriva_set_* function has this shape: bool(ctx, const char*, char**).
using SetterFn = bool (*)(HayagrivaCtx*, const char*, char**);

void call_setter(SetterFn fn, HayagrivaCtx* ctx, const char* arg, const char* what) {
    char* err = nullptr;
    bool ok = fn(ctx, arg, &err);
    CStrPtr errPtr(err);
    if (!ok) {
        throw std::runtime_error(
            std::string(what) + ": " + (errPtr ? errPtr.get() : "unknown error"));
    }
}

std::string read_file(const std::string& path) {
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        throw std::runtime_error("failed to open `" + path + "`");
    }
    std::ostringstream contents;
    contents << file.rdbuf();
    return contents.str();
}

std::string usage(const char* argv0) {
    return std::string("usage: ") + argv0
           + " [--list-styles] [--list-locales] [--list-entries --bib <file.bib>] [--bib <file.bib>] [--style <style-name>] [--locale <locale>] [--cite <file.txt>]";
}

Config parse_args(int argc, char** argv) {
    Config config;
    CLI::App app("C++ example for hayagriva-ffi");
    app.set_help_all_flag("--help-all", "Show all help");
    app.footer(usage(argv[0]));

    app.add_flag("--list-styles", config.list_styles_only, "List available styles");
    app.add_flag("--list-locales", config.list_locales_only, "List available locales");
    app.add_flag("--list-entries", config.list_entries_only, "List bibliography entries");
    app.add_option("--bib", config.bib_path, "Path to the BibLaTeX bibliography");
    app.add_option("--style", config.style_key, "Citation style key");
    app.add_option("--locale", config.locale_storage, "Locale override");
    app.add_option("--cite", config.cite_path, "Path to citation groups file");

    try {
        app.parse(argc, argv);
    } catch (const CLI::CallForHelp&) {
        std::ostringstream out;
        out << app.help();
        throw std::runtime_error(out.str());
    } catch (const CLI::ParseError& e) {
        throw std::runtime_error(e.what());
    }

    if (config.locale_storage) {
        config.locale = config.locale_storage->c_str();
    }

    if (config.list_styles_only || config.list_locales_only) {
        return config;
    }

    if (config.list_entries_only) {
        if (config.bib_path.empty()) {
            throw std::runtime_error("missing required argument: --bib");
        }
        return config;
    }

    if (config.bib_path.empty()) {
        throw std::runtime_error("missing required argument: --bib");
    }
    if (config.cite_path.empty()) {
        throw std::runtime_error("missing required argument: --cite");
    }

    return config;
}

std::vector<std::vector<std::string>> read_citation_groups(const std::string& path) {
    std::istringstream input(read_file(path));
    std::vector<std::vector<std::string>> groups;
    std::string raw_line;
    int line_no = 0;

    while (std::getline(input, raw_line)) {
        ++line_no;
        std::string line = raw_line;

        size_t start = line.find_first_not_of(" \t\r");
        if (start == std::string::npos) continue;
        size_t end = line.find_last_not_of(" \t\r");
        line = line.substr(start, end - start + 1);
        if (line.empty() || line[0] == '#') continue;

        std::vector<std::string> items;
        if (line.find(',') != std::string::npos) {
            std::stringstream ss(line);
            std::string item;
            while (std::getline(ss, item, ',')) {
                size_t item_start = item.find_first_not_of(" \t\r");
                if (item_start == std::string::npos) {
                    throw std::runtime_error(
                        "invalid citation group at " + path + ":" + std::to_string(line_no));
                }
                size_t item_end = item.find_last_not_of(" \t\r");
                item = item.substr(item_start, item_end - item_start + 1);
                if (item.empty()) {
                    throw std::runtime_error(
                        "invalid citation group at " + path + ":" + std::to_string(line_no));
                }
                items.push_back(item);
            }
        } else {
            std::stringstream ss(line);
            std::string item;
            while (ss >> item) items.push_back(item);
        }

        if (items.empty()) {
            throw std::runtime_error(
                "invalid citation group at " + path + ":" + std::to_string(line_no));
        }

        groups.push_back(std::move(items));
    }

    return groups;
}

// Renders a run with the subset of styles terminals can reasonably show via
// ANSI escape sequences. Unsupported fields (e.g. small-caps, sup/sub) fall
// back to plain text. A real LibreOffice UNO layer would instead map these
// fields onto CharPosture/CharWeight/CharEscapement/CharUnderline/
// CharCaseMap/HyperLinkURL and insert the run as rich text.
std::string describe_run(const json& run) {
    std::string text = run.at("text").get<std::string>();
    std::string sgr;

    if (run.at("italic").get<bool>()) sgr += "\x1b[3m";

    std::string font_weight = run.at("font_weight").get<std::string>();
    if (font_weight == "bold") {
        sgr += "\x1b[1m";
    } else if (font_weight == "light") {
        sgr += "\x1b[2m";
    }

    if (run.at("underline").get<bool>()) sgr += "\x1b[4m";

    if (!run.at("url").is_null()) {
        const std::string url = run.at("url").get<std::string>();
        return "\x1b]8;;" + url + "\x1b\\" + (sgr.empty() ? text : sgr + text + "\x1b[0m")
               + "\x1b]8;;\x1b\\";
    }

    if (sgr.empty()) return text;
    return sgr + text + "\x1b[0m";
}

std::string describe_runs(const json& runs) {
    std::string out;
    for (const auto& run : runs) out += describe_run(run);
    return out;
}

std::string join_authors(const json& authors) {
    std::string out;
    for (size_t i = 0; i < authors.size(); ++i) {
        if (i > 0) out += "; ";
        out += authors[i].get<std::string>();
    }
    return out;
}

std::string json_string_or_empty(const json& object, const char* key) {
    auto it = object.find(key);
    if (it == object.end() || it->is_null()) return "";
    return it->get<std::string>();
}

std::string json_int_or_empty(const json& object, const char* key) {
    auto it = object.find(key);
    if (it == object.end() || it->is_null()) return "";
    return std::to_string(it->get<int>());
}

} // namespace

int main(int argc, char** argv) {
    try {
        Config config = parse_args(argc, argv);

        if (config.list_styles_only) {
            CStrPtr styles_json(hayagriva_list_styles());
            json styles = json::parse(styles_json.get());
            std::cout << "Available styles (" << styles.size() << "):\n";
            for (const auto& style : styles) {
                std::cout << "  " << style.at("key").get<std::string>() << " -- "
                          << style.at("display_name").get<std::string>();
                if (style.contains("default_locale") && !style.at("default_locale").is_null()) {
                    std::cout << " [default locale: "
                              << style.at("default_locale").get<std::string>() << "]";
                }
                std::cout << "\n";
            }
            std::cout << "\n";
            return 0;
        }

        if (config.list_locales_only) {
            CStrPtr locales_json(hayagriva_list_locales());
            json locales = json::parse(locales_json.get());
            std::cout << "Available locales (" << locales.size() << "):\n";
            for (const auto& locale : locales) {
                std::cout << "  " << locale.get<std::string>() << "\n";
            }
            std::cout << "\n";
            return 0;
        }

        CtxPtr ctx(hayagriva_new(nullptr));
        if (!ctx) throw std::runtime_error("hayagriva_new failed");

        std::string bib_str = read_file(config.bib_path);
        call_setter(hayagriva_set_bib, ctx.get(), bib_str.c_str(), "set_bib");

        if (config.list_entries_only) {
            CStrPtr entries_json(hayagriva_list_entries(ctx.get()));
            if (!entries_json) throw std::runtime_error("hayagriva_list_entries failed");
            json entries = json::parse(entries_json.get());
            std::cout << "Bibliography entries (" << entries.size() << "):\n";
            for (const auto& entry : entries) {
                std::cout << "  [" << entry.at("key").get<std::string>() << "]\n";
                std::cout << "    title: " << json_string_or_empty(entry, "title") << "\n";
                std::cout << "    authors: "
                          << (entry.contains("authors") ? join_authors(entry.at("authors")) : "")
                          << "\n";
                std::cout << "    year: " << json_int_or_empty(entry, "year") << "\n";
                std::cout << "    container_title: "
                          << json_string_or_empty(entry, "container_title") << "\n";
                std::cout << "    volume: " << json_string_or_empty(entry, "volume") << "\n";
                std::cout << "    issue: " << json_string_or_empty(entry, "issue") << "\n";
                std::cout << "    page_range: " << json_string_or_empty(entry, "page_range")
                          << "\n";
            }
            std::cout << "\n";
            return 0;
        }

        call_setter(hayagriva_set_style, ctx.get(), config.style_key.c_str(), "set_style");
        call_setter(hayagriva_set_locale, ctx.get(), config.locale, "set_locale");

        std::vector<std::vector<std::string>> citation_groups =
            read_citation_groups(config.cite_path);
        if (citation_groups.empty()) {
            std::cerr << "citation text file does not contain any citation groups\n";
            return 1;
        }

        std::map<int, std::vector<std::string>> slide_citations;
        json citation_groups_json = json::array();
        for (size_t i = 0; i < citation_groups.size(); ++i) {
            slide_citations[static_cast<int>(i + 1)] = citation_groups[i];
            citation_groups_json.push_back(json::array());
            for (const auto& key : citation_groups[i]) {
                citation_groups_json.back().push_back(key);
            }
        }

        char* render_err = nullptr;
        CStrPtr render_json_ptr(hayagriva_render(
            ctx.get(), citation_groups_json.dump().c_str(), &render_err));
        CStrPtr render_err_ptr(render_err);
        if (!render_json_ptr) {
            throw std::runtime_error(
                std::string("hayagriva_render: ")
                + (render_err_ptr ? render_err_ptr.get() : "unknown error"));
        }
        json render = json::parse(render_json_ptr.get());

        std::cout << "Rendered citations:\n";
        const json& citations = render.at("citations");
        for (size_t i = 0; i < citations.size(); ++i) {
            std::cout << "  citation " << i << ": " << citations[i].at("text").get<std::string>()
                       << (citations[i].at("sup").get<bool>() ? "  (superscript)" : "") << "\n";
        }
        std::cout << "\n";

        // Index bibliography items by key so slide footers can look up just
        // the entries they used, independent of the global bibliography's
        // own ordering.
        std::map<std::string, json> bib_by_key;
        for (const auto& item : render.at("bibliography").at("items")) {
            bib_by_key[item.at("key").get<std::string>()] = item;
        }

        std::cout << "Per-slide bibliography footers:\n";
        for (const auto& [slide, keys] : slide_citations) {
            std::cout << "  Slide " << slide << ":\n";
            for (const auto& key : keys) {
                const json& item = bib_by_key.at(key);
                std::cout << "    " << describe_runs(item.at("prefix_runs"))
                           << describe_runs(item.at("runs")) << "\n";
            }
        }
        std::cout << "\n";

        std::cout << "Bibliography:\n";
        for (const auto& item : render.at("bibliography").at("items")) {
            std::cout << "  " << describe_runs(item.at("prefix_runs"))
                      << describe_runs(item.at("runs")) << "\n";
        }
    } catch (const std::exception& ex) {
        std::string message = ex.what();
        if (message.rfind("usage: ", 0) == 0) {
            std::cout << message << "\n";
            return 0;
        }
        std::cerr << "error: " << message << "\n";
        std::cerr << usage(argv[0]) << "\n";
        return 2;
    }

    return 0;
}
