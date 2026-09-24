cmake_minimum_required(VERSION 3.21)
project(NexusOfficeSuite VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 Svg Test)
find_package(PkgConfig REQUIRED)
pkg_check_modules(LIBZIP REQUIRED IMPORTED_TARGET libzip)
pkg_check_modules(ENCHANT QUIET IMPORTED_TARGET enchant-2)

add_library(nexus-core STATIC
    src/AppController.cpp
    src/AppController.h
    src/ThemeManager.cpp
    src/ThemeManager.h
    src/WriteDocument.cpp
    src/WriteTemplates.cpp
    src/WriteDocx.cpp
    src/WriteDocument.h
    src/SheetsDocument.cpp
    src/SheetsDocumentFeatures.cpp
    src/SheetsDocumentHistory.cpp
    src/SheetsWorkbook.cpp
    src/SheetsTemplates.cpp
    src/SheetsXlsx.cpp
    src/SheetsDocument.h
    src/PresentDocument.cpp
    src/PresentDocumentHistory.cpp
    src/PresentElements.cpp
    src/PresentTemplates.cpp
    src/PresentPptx.cpp
    src/PresentDocument.h
)
target_link_libraries(nexus-core PUBLIC Qt6::Core Qt6::Gui Qt6::Quick PkgConfig::LIBZIP)
if(ENCHANT_FOUND)
    target_link_libraries(nexus-core PUBLIC PkgConfig::ENCHANT)
    target_compile_definitions(nexus-core PUBLIC NEXUS_HAVE_ENCHANT)
endif()
target_include_directories(nexus-core PUBLIC src)

qt_add_executable(nexus-office src/main.cpp src/IconProvider.cpp src/IconProvider.h)
target_link_libraries(nexus-office PRIVATE nexus-core Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::Svg)
target_compile_definitions(nexus-office PRIVATE NEXUS_VERSION="${PROJECT_VERSION}")
file(GLOB NEXUS_ICON_FILES RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}" CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/assets/icons/*.svg")
qt_add_resources(nexus-office "nexus_resources" PREFIX "/" FILES
    qml/Main.qml
    qml/components/ShellButton.qml
    qml/components/NavItem.qml
    qml/components/ThemeTile.qml
    qml/components/IconView.qml
    qml/components/IconButton.qml
    qml/components/AppMenuButton.qml
    qml/components/AppMenuItem.qml
    qml/components/StartCard.qml
    qml/components/TemplateCard.qml
    qml/WriteEditor.qml
    qml/SheetsEditor.qml
    qml/PresentEditor.qml
    qml/PresentShape.qml
    qml/PresentationSlide.qml
    qml/SlideshowWindow.qml
    assets/themes.json
    ${NEXUS_ICON_FILES}
)

include(CTest)
if(BUILD_TESTING)
    add_executable(theme-tests tests/ThemeTests.cpp)
    target_link_libraries(theme-tests PRIVATE nexus-core Qt6::Test)
    qt_add_resources(theme-tests "theme_test_resources" PREFIX "/" FILES assets/themes.json)
    add_test(NAME theme-tests COMMAND theme-tests)

    add_executable(write-tests tests/WriteTests.cpp)
    target_link_libraries(write-tests PRIVATE nexus-core Qt6::Test)
    add_test(NAME write-tests COMMAND write-tests)

    add_executable(sheets-tests tests/SheetsTests.cpp)
    target_link_libraries(sheets-tests PRIVATE nexus-core Qt6::Test)
    add_test(NAME sheets-tests COMMAND sheets-tests)

    add_executable(present-tests tests/PresentTests.cpp)
    target_link_libraries(present-tests PRIVATE nexus-core Qt6::Test)
    add_test(NAME present-tests COMMAND present-tests)

    add_executable(icon-tests tests/IconTests.cpp src/IconProvider.cpp src/IconProvider.h)
    target_include_directories(icon-tests PRIVATE src)
    target_link_libraries(icon-tests PRIVATE Qt6::Core Qt6::Gui Qt6::Quick Qt6::Svg Qt6::Test)
    qt_add_resources(icon-tests "icon_test_resources" PREFIX "/" FILES assets/icons/file-text.svg)
    add_test(NAME icon-tests COMMAND icon-tests)
endif()

install(TARGETS nexus-office RUNTIME DESTINATION bin)
install(PROGRAMS packaging/nexus-write packaging/nexus-sheets packaging/nexus-present DESTINATION bin)
install(FILES packaging/nexus-write.desktop packaging/nexus-sheets.desktop packaging/nexus-present.desktop
        DESTINATION share/applications)
install(FILES packaging/nexus-office.xml DESTINATION share/mime/packages)
install(FILES assets/icons/LICENSE DESTINATION share/doc/nexus-office RENAME lucide-LICENSE)
